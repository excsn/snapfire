
use crate::fingerprint::{canonical_f32_bits, canonical_f64_bits};

/// Keys reach a `ValueMap` from a request body through
/// `snapfire_fsr_payload::json_to_value`, so the hasher is seeded per instance
/// and a caller must not swap in a fixed-state one.
pub type ValueHasher = foldhash::fast::RandomState;

pub use crate::fields::{Fields, Key};

/// What a `ValueStr` holds. Changing this changes the representation of every
/// string in the value model in one line, without reaching any consumer.
pub type Text = smol_str::SmolStr;

/// The string of the value model, behind a name so its representation is one
/// crate's business. Reads go through `Deref<Target = str>`, so a consumer
/// sees a string slice and does not care what is underneath.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueStr(Text);

impl ValueStr {
  pub fn as_str(&self) -> &str {
    &self.0
  }

  pub fn into_text(self) -> Text {
    self.0
  }

  /// This string for the life of the process: a clone copies a pointer and writes nothing shared.
  pub fn frozen(&self) -> ValueStr {
    if self.0.is_heap_allocated() {
      ValueStr(Text::new_static(Box::leak(self.0.as_str().into())))
    } else {
      self.clone()
    }
  }
}

impl std::ops::Deref for ValueStr {
  type Target = str;
  fn deref(&self) -> &str {
    &self.0
  }
}

impl std::fmt::Debug for ValueStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(f)
  }
}

impl std::fmt::Display for ValueStr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    std::fmt::Display::fmt(&self.0, f)
  }
}

impl std::borrow::Borrow<str> for ValueStr {
  fn borrow(&self) -> &str {
    &self.0
  }
}

impl AsRef<str> for ValueStr {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl From<String> for ValueStr {
  fn from(v: String) -> Self {
    ValueStr(Text::from(v))
  }
}

impl From<&String> for ValueStr {
  fn from(v: &String) -> Self {
    ValueStr(Text::from(v.as_str()))
  }
}

impl From<&str> for ValueStr {
  fn from(v: &str) -> Self {
    ValueStr(Text::from(v))
  }
}

impl From<std::borrow::Cow<'_, str>> for ValueStr {
  fn from(v: std::borrow::Cow<'_, str>) -> Self {
    ValueStr(Text::from(v))
  }
}

impl From<ValueStr> for String {
  fn from(v: ValueStr) -> Self {
    v.0.into()
  }
}

impl PartialEq<str> for ValueStr {
  fn eq(&self, other: &str) -> bool {
    &*self.0 == other
  }
}

impl PartialEq<&str> for ValueStr {
  fn eq(&self, other: &&str) -> bool {
    &*self.0 == *other
  }
}

impl PartialEq<String> for ValueStr {
  fn eq(&self, other: &String) -> bool {
    &*self.0 == other.as_str()
  }
}

impl PartialEq<ValueStr> for str {
  fn eq(&self, other: &ValueStr) -> bool {
    self == &*other.0
  }
}

impl PartialEq<ValueStr> for &str {
  fn eq(&self, other: &ValueStr) -> bool {
    *self == &*other.0
  }
}

/// How deep dropping a value recurses before what it releases waits on a list.
const DROP_DEPTH: usize = 64;

std::thread_local! {
  static DROPPING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
  static WAITING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
  static RELEASED: std::cell::RefCell<Vec<Released>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The last reference to an object or an array, which a drop lets go of.
enum Released {
  Map { _held: std::sync::Arc<Place<Fields>> },
  Seq { _held: std::sync::Arc<Place<Items>> },
}

/// Where an object lives. Holders read it directly. [`ValueMap::through`] and [`ValueSeq::through`] change it for all of them: the interpreter calls those on one thread with no reference into the object alive across the write. Independent `let`s that run on other threads never write.
struct Place<T> {
  value: std::cell::UnsafeCell<T>,
  kind: std::cell::Cell<Collection>,
}

unsafe impl<T: Send> Send for Place<T> {}
unsafe impl<T: Send + Sync> Sync for Place<T> {}

/// Which JavaScript collection an object or an array is: a plain one, a `Map` kept as an object of its keys or a `Set` kept as an array of its items. Equality, the fingerprint and the wire ignore it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Collection {
  #[default]
  Plain,
  Map,
  Set,
  /// An `Error` or one of its subclasses: an object whose `name`, `message`, `cause` and `errors` JavaScript does not enumerate.
  Error,
  /// An object of the platform's whose fields JavaScript does not enumerate, such as an `Intl.Locale` or a `DisposableStack`: JSON writes it as `{}`.
  Opaque,
  /// A typed array of the element named, kept as an array of its items already converted to it. A write converts what it writes and JSON writes the items as an object of their indexes.
  Typed(TypedElement),
}

/// The element a typed array holds, as its constructor names it without `Array`: `Int16` for `Int16Array`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedElement {
  Int8,
  Uint8,
  Uint8Clamped,
  Int16,
  Uint16,
  Int32,
  Uint32,
  Float16,
  Float32,
  Float64,
  BigInt64,
  BigUint64,
}

impl TypedElement {
  pub const ALL: [TypedElement; 12] = [TypedElement::Int8, TypedElement::Uint8, TypedElement::Uint8Clamped, TypedElement::Int16, TypedElement::Uint16, TypedElement::Int32, TypedElement::Uint32, TypedElement::Float16, TypedElement::Float32, TypedElement::Float64, TypedElement::BigInt64, TypedElement::BigUint64];

  /// The element's name, `Int16`.
  pub fn name(self) -> &'static str {
    match self {
      TypedElement::Int8 => "Int8",
      TypedElement::Uint8 => "Uint8",
      TypedElement::Uint8Clamped => "Uint8Clamped",
      TypedElement::Int16 => "Int16",
      TypedElement::Uint16 => "Uint16",
      TypedElement::Int32 => "Int32",
      TypedElement::Uint32 => "Uint32",
      TypedElement::Float16 => "Float16",
      TypedElement::Float32 => "Float32",
      TypedElement::Float64 => "Float64",
      TypedElement::BigInt64 => "BigInt64",
      TypedElement::BigUint64 => "BigUint64",
    }
  }

  /// The element `name` names, `None` for any other name.
  pub fn of(name: &str) -> Option<TypedElement> {
    TypedElement::ALL.into_iter().find(|e| e.name() == name)
  }

  /// The bytes one element takes.
  pub fn bytes(self) -> usize {
    match self {
      TypedElement::Int8 | TypedElement::Uint8 | TypedElement::Uint8Clamped => 1,
      TypedElement::Int16 | TypedElement::Uint16 | TypedElement::Float16 => 2,
      TypedElement::Int32 | TypedElement::Uint32 | TypedElement::Float32 => 4,
      TypedElement::Float64 | TypedElement::BigInt64 | TypedElement::BigUint64 => 8,
    }
  }
}

impl<T> Place<T> {
  fn new(value: T, kind: Collection) -> Self {
    Place { value: std::cell::UnsafeCell::new(value), kind: std::cell::Cell::new(kind) }
  }

  #[inline]
  fn get(&self) -> &T {
    unsafe { &*self.value.get() }
  }
}

fn release(value: Released) {
  let depth = DROPPING.get();
  if depth >= DROP_DEPTH {
    let _ = RELEASED.try_with(|list| list.borrow_mut().push(value));
    WAITING.set(true);
    return;
  }
  DROPPING.set(depth + 1);
  drop(value);
  if depth == 0 {
    while WAITING.get() {
      match RELEASED.try_with(|list| list.borrow_mut().pop()) {
        Ok(Some(next)) => drop(next),
        _ => WAITING.set(false),
      }
    }
  }
  DROPPING.set(depth);
}

/// What an object or an array keeps behind its reference.
trait Part: Clone + Default + 'static {
  fn released(held: std::sync::Arc<Place<Self>>) -> Released;
}

impl Part for Fields {
  fn released(held: std::sync::Arc<Place<Self>>) -> Released {
    Released::Map { _held: held }
  }
}

impl Part for Items {
  fn released(held: std::sync::Arc<Place<Self>>) -> Released {
    Released::Seq { _held: held }
  }
}

/// An object's or an array's storage. A shared one is counted and its drop cannot recurse as deep as the value is nested. A frozen one lives as long as the process and is read by every thread with no write to a shared word: cloning it copies a pointer and dropping it does nothing.
enum Held<T: Part> {
  Shared(std::mem::ManuallyDrop<std::sync::Arc<Place<T>>>),
  Frozen(&'static Place<T>),
}

impl<T: Part> Held<T> {
  fn new(value: T) -> Self {
    Self::of_kind(value, Collection::Plain)
  }

  fn of_kind(value: T, kind: Collection) -> Self {
    Held::Shared(std::mem::ManuallyDrop::new(std::sync::Arc::new(Place::new(value, kind))))
  }

  #[inline]
  fn place(&self) -> &Place<T> {
    match self {
      Held::Shared(held) => held,
      Held::Frozen(held) => held,
    }
  }

  #[inline]
  fn get(&self) -> &T {
    self.place().get()
  }

  #[inline]
  fn kind(&self) -> Collection {
    self.place().kind.get()
  }

  /// This storage as a collection of `kind`: marked where it stands when nothing else holds it, a copy otherwise.
  fn with_kind(self, kind: Collection) -> Self {
    match &self {
      Held::Shared(held) if std::sync::Arc::strong_count(held) == 1 => {
        held.kind.set(kind);
        self
      }
      _ => Self::of_kind(self.get().clone(), kind),
    }
  }

  fn make_mut(&mut self) -> &mut T {
    let sole = matches!(self, Held::Shared(held) if std::sync::Arc::strong_count(held) == 1);
    if !sole {
      *self = Self::of_kind(self.get().clone(), self.kind());
    }
    match self {
      Held::Shared(held) => unsafe { &mut *held.value.get() },
      Held::Frozen(_) => unreachable!("just copied"),
    }
  }

  fn is_frozen(&self) -> bool {
    matches!(self, Held::Frozen(_))
  }

  /// How many holders share a counted object. A frozen one has no count.
  fn holders(&self) -> usize {
    match self {
      Held::Shared(held) => std::sync::Arc::strong_count(held),
      Held::Frozen(_) => usize::MAX,
    }
  }

  /// The object itself for every holder. `None` for a frozen one, which requests share.
  #[allow(clippy::mut_from_ref)]
  fn through(&self) -> Option<&mut T> {
    match self {
      Held::Shared(held) => Some(unsafe { &mut *held.value.get() }),
      Held::Frozen(_) => None,
    }
  }

  fn same(&self, other: &Self) -> bool {
    std::ptr::eq(self.get(), other.get())
  }

  fn into_inner(self) -> T {
    let mut this = std::mem::ManuallyDrop::new(self);
    match &mut *this {
      Held::Shared(held) => {
        let held = unsafe { std::mem::ManuallyDrop::take(held) };
        match std::sync::Arc::try_unwrap(held) {
          Ok(place) => place.value.into_inner(),
          Err(held) => held.get().clone(),
        }
      }
      Held::Frozen(held) => held.get().clone(),
    }
  }
}

impl<T: Part> Clone for Held<T> {
  #[inline]
  fn clone(&self) -> Self {
    match self {
      Held::Shared(held) => Held::Shared(held.clone()),
      Held::Frozen(held) => Held::Frozen(held),
    }
  }
}

impl<T: Part> Default for Held<T> {
  fn default() -> Self {
    Held::new(T::default())
  }
}

impl<T: Part> Drop for Held<T> {
  #[inline]
  fn drop(&mut self) {
    if let Held::Shared(held) = self {
      let held = unsafe { std::mem::ManuallyDrop::take(held) };
      if std::sync::Arc::strong_count(&held) == 1 {
        release(T::released(held));
      }
    }
  }
}

impl Value {
  /// This value and all it holds frozen for the life of the process. For what a plan keeps: a constant every request reads. Each object and array in it is noted against the whole, which [`Value::frozen_root`] answers.
  pub fn frozen(&self) -> Value {
    let mut nodes = Vec::new();
    let root = self.freeze_into(&mut nodes);
    if !nodes.is_empty() {
      let mut roots = FROZEN_ROOTS.write();
      for node in nodes {
        roots.insert(node, root.clone());
      }
    }
    root
  }

  fn freeze_into(&self, nodes: &mut Vec<usize>) -> Value {
    let frozen = match self {
      Value::Map(map) => {
        let fields: Fields = map.iter().map(|(k, v)| (if k.is_heap_allocated() { Key::new_static(Box::leak(k.as_str().into())) } else { k.clone() }, crate::on_enough_stack(|| v.freeze_into(nodes)))).collect();
        Value::Map(ValueMap(Held::Frozen(Box::leak(Box::new(Place::new(fields, map.kind()))))))
      }
      Value::Seq(items) => {
        let frozen: Items = items.iter().map(|v| crate::on_enough_stack(|| v.freeze_into(nodes))).collect();
        Value::Seq(ValueSeq(Held::Frozen(Box::leak(Box::new(Place::new(frozen, items.kind()))))))
      }
      Value::Str(text) => return Value::Str(text.frozen()),
      other => return other.clone(),
    };
    nodes.push(frozen.address().expect("an object or an array"));
    frozen
  }

  /// The frozen value a frozen object or array was made part of, so a body about to write one can make its own copy of the whole.
  pub fn frozen_root(&self) -> Option<Value> {
    match self {
      Value::Map(map) if map.is_frozen() => FROZEN_ROOTS.read().get(&map.addr()).cloned(),
      Value::Seq(items) if items.is_frozen() => FROZEN_ROOTS.read().get(&items.addr()).cloned(),
      _ => None,
    }
  }

  /// The address of an object or an array, its identity while it lives.
  pub fn address(&self) -> Option<usize> {
    match self {
      Value::Map(map) => Some(map.addr()),
      Value::Seq(items) => Some(items.addr()),
      _ => None,
    }
  }
}

/// Whether `value` reaches the object or array at `target` through what it holds, looking at no more than `budget` objects and arrays or 16 levels down. `None` when it gave up first. A frozen value holds nothing a request made, so the search stops there.
pub fn reaches(value: &Value, target: usize, budget: usize) -> Option<bool> {
  let mut left = budget;
  reach(value, target, &mut left, 0)
}

fn reach(value: &Value, target: usize, left: &mut usize, depth: usize) -> Option<bool> {
  let children: &mut dyn Iterator<Item = &Value> = match value {
    Value::Map(map) if !map.is_frozen() => &mut map.values(),
    Value::Seq(items) if !items.is_frozen() => &mut items.iter(),
    _ => return Some(false),
  };
  if value.address() == Some(target) {
    return Some(true);
  }
  if *left == 0 || depth == 16 {
    return None;
  }
  *left -= 1;
  let mut gave_up = false;
  let own = value.address();
  for child in children {
    if child.address().is_some() && child.address() == own {
      continue;
    }
    match reach(child, target, left, depth + 1) {
      Some(true) => return Some(true),
      Some(false) => {}
      None => gave_up = true,
    }
  }
  if gave_up { None } else { Some(false) }
}

/// Frees the objects and arrays reachable from `candidates` that nothing holds but each other: a cycle a write made and the request no longer reaches. Each object reached is counted against the references the others hold to it; one held from outside lives with all it reaches. The rest are emptied so their counts fall to zero. Call it where nothing borrows a value reached from `candidates`, at the end of a request, after everything else the request held is gone.
pub fn collect_cycles(candidates: Vec<Value>) {
  if candidates.is_empty() {
    return;
  }
  let counted = |value: &Value| match value {
    Value::Map(map) => !map.is_frozen(),
    Value::Seq(items) => !items.is_frozen(),
    _ => false,
  };
  let mut slot: std::collections::HashMap<usize, usize, ValueHasher> = std::collections::HashMap::with_capacity_and_hasher(candidates.len() * 4, Default::default());
  let mut nodes: Vec<Value> = Vec::with_capacity(candidates.len() * 4);
  let mut edges: Vec<(usize, usize)> = Vec::with_capacity(candidates.len() * 4);
  for candidate in candidates {
    if counted(&candidate) {
      let addr = candidate.address().expect("an object or an array");
      if let std::collections::hash_map::Entry::Vacant(free) = slot.entry(addr) {
        free.insert(nodes.len());
        nodes.push(candidate);
      }
    }
  }
  let mut at = 0;
  while at < nodes.len() {
    let node = nodes[at].clone();
    let children: &mut dyn Iterator<Item = &Value> = match &node {
      Value::Map(map) => &mut map.values(),
      Value::Seq(items) => &mut items.iter(),
      _ => unreachable!("only counted objects and arrays are taken"),
    };
    for child in children.filter(|v| counted(v)) {
      let addr = child.address().expect("an object or an array");
      let to = *slot.entry(addr).or_insert_with(|| {
        nodes.push(child.clone());
        nodes.len() - 1
      });
      edges.push((at, to));
    }
    at += 1;
  }
  drop(slot);
  let mut inside = vec![0usize; nodes.len()];
  let mut start = vec![0usize; nodes.len() + 1];
  for (from, to) in &edges {
    inside[*to] += 1;
    start[*from + 1] += 1;
  }
  for at in 0..nodes.len() {
    start[at + 1] += start[at];
  }
  let mut fill = start.clone();
  let mut targets = vec![0usize; edges.len()];
  for (from, to) in &edges {
    targets[fill[*from]] = *to;
    fill[*from] += 1;
  }
  let holders = |value: &Value| match value {
    Value::Map(map) => map.0.holders(),
    Value::Seq(items) => items.0.holders(),
    _ => usize::MAX,
  };
  let mut live = vec![false; nodes.len()];
  let mut marking: Vec<usize> = (0..nodes.len()).filter(|at| holders(&nodes[*at]) > inside[*at] + 1).collect();
  while let Some(at) = marking.pop() {
    if std::mem::replace(&mut live[at], true) {
      continue;
    }
    marking.extend(targets[start[at]..start[at + 1]].iter().copied().filter(|to| !live[*to]));
  }
  let mut emptied: Vec<Value> = Vec::new();
  for (at, node) in nodes.iter().enumerate() {
    if live[at] {
      continue;
    }
    match node {
      Value::Map(map) => {
        if let Some(fields) = map.through() {
          emptied.extend(std::mem::take(fields).into_values());
        }
      }
      Value::Seq(items) => {
        if let Some(held) = items.through() {
          emptied.append(held);
        }
      }
      _ => {}
    }
  }
  drop(emptied);
  drop(nodes);
}

/// Every frozen object and array, by address, with the frozen value it was made part of. Written when a value is frozen and read only when a body writes one.
static FROZEN_ROOTS: std::sync::LazyLock<parking_lot::RwLock<std::collections::HashMap<usize, Value, ValueHasher>>> = std::sync::LazyLock::new(Default::default);

/// Copy on write: cloning shares and the first `&mut` after a share copies.
/// A render clones a props map at every component and every loop iteration
/// without reading it back, which is what the sharing is for.
#[derive(Clone, Default)]
pub struct ValueMap(Held<Fields>);

pub type Props = ValueMap;

pub type Items = Vec<Value>;

/// The sequence of the value model, behind a name so its representation is one
/// crate's business, the way [`ValueStr`] is. Copy on write as [`ValueMap`] is,
/// so cloning an array does not recurse into what it holds.
#[derive(Clone, Default)]
pub struct ValueSeq(Held<Items>);

impl ValueSeq {
  pub fn kind(&self) -> Collection {
    self.0.kind()
  }

  /// How many holders share this array; `usize::MAX` for a frozen one.
  pub fn holders(&self) -> usize {
    self.0.holders()
  }

  /// This array as a collection of `kind`, a `Set` for one.
  pub fn with_kind(self, kind: Collection) -> Self {
    ValueSeq(self.0.with_kind(kind))
  }

  pub fn is_frozen(&self) -> bool {
    self.0.is_frozen()
  }

  #[allow(clippy::mut_from_ref)]
  pub fn through(&self) -> Option<&mut Items> {
    self.0.through()
  }

  pub fn same(&self, other: &Self) -> bool {
    self.0.same(&other.0)
  }

  pub fn addr(&self) -> usize {
    self.0.get() as *const Items as usize
  }

  pub fn into_items(self) -> Items {
    self.0.into_inner()
  }
}

impl std::ops::Deref for ValueSeq {
  type Target = Items;
  fn deref(&self) -> &Items {
    self.0.get()
  }
}

impl std::ops::DerefMut for ValueSeq {
  fn deref_mut(&mut self) -> &mut Items {
    self.0.make_mut()
  }
}

impl std::fmt::Debug for ValueSeq {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.get().fmt(f)
  }
}

impl PartialEq for ValueSeq {
  fn eq(&self, other: &Self) -> bool {
    *self.0.get() == *other.0.get()
  }
}

impl PartialEq<Items> for ValueSeq {
  fn eq(&self, other: &Items) -> bool {
    *self.0.get() == *other
  }
}

impl PartialEq<Fields> for ValueMap {
  fn eq(&self, other: &Fields) -> bool {
    *self.0.get() == *other
  }
}

impl From<Items> for ValueSeq {
  fn from(items: Items) -> Self {
    ValueSeq(Held::new(items))
  }
}

impl FromIterator<Value> for ValueSeq {
  fn from_iter<T: IntoIterator<Item = Value>>(iter: T) -> Self {
    ValueSeq(Held::new(Items::from_iter(iter)))
  }
}

impl IntoIterator for ValueSeq {
  type Item = Value;
  type IntoIter = std::vec::IntoIter<Value>;
  fn into_iter(self) -> Self::IntoIter {
    self.into_items().into_iter()
  }
}

impl<'a> IntoIterator for &'a ValueSeq {
  type Item = &'a Value;
  type IntoIter = std::slice::Iter<'a, Value>;
  fn into_iter(self) -> Self::IntoIter {
    self.0.get().iter()
  }
}

impl<'a> IntoIterator for &'a mut ValueSeq {
  type Item = &'a mut Value;
  type IntoIter = std::slice::IterMut<'a, Value>;
  fn into_iter(self) -> Self::IntoIter {
    self.0.make_mut().iter_mut()
  }
}

impl Extend<Value> for ValueSeq {
  fn extend<T: IntoIterator<Item = Value>>(&mut self, iter: T) {
    self.0.make_mut().extend(iter);
  }
}

impl ValueMap {
  pub fn kind(&self) -> Collection {
    self.0.kind()
  }

  /// How many holders share this object; `usize::MAX` for a frozen one.
  pub fn holders(&self) -> usize {
    self.0.holders()
  }

  /// This object as a collection of `kind`, a `Map` for one.
  pub fn with_kind(self, kind: Collection) -> Self {
    ValueMap(self.0.with_kind(kind))
  }

  pub fn is_frozen(&self) -> bool {
    self.0.is_frozen()
  }

  #[allow(clippy::mut_from_ref)]
  pub fn through(&self) -> Option<&mut Fields> {
    self.0.through()
  }

  pub fn same(&self, other: &Self) -> bool {
    self.0.same(&other.0)
  }

  pub fn addr(&self) -> usize {
    self.0.get() as *const Fields as usize
  }

  pub fn into_fields(self) -> Fields {
    self.0.into_inner()
  }

  /// [`Self::into_fields`] with room for `room` more fields: taken where it stands when nothing else holds it, copied once at that size otherwise.
  pub fn into_fields_with_room(self, room: usize) -> Fields {
    if self.holders() == 1 {
      let mut fields = self.into_fields();
      fields.reserve(room);
      return fields;
    }
    self.0.get().copy_with_room(room)
  }
}

impl std::ops::Deref for ValueMap {
  type Target = Fields;
  fn deref(&self) -> &Fields {
    self.0.get()
  }
}

impl std::ops::DerefMut for ValueMap {
  fn deref_mut(&mut self) -> &mut Fields {
    self.0.make_mut()
  }
}

impl std::fmt::Debug for ValueMap {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.get().fmt(f)
  }
}

impl PartialEq for ValueMap {
  fn eq(&self, other: &Self) -> bool {
    self.0.same(&other.0) || *self.0.get() == *other.0.get()
  }
}

impl From<Fields> for ValueMap {
  fn from(fields: Fields) -> Self {
    ValueMap(Held::new(fields))
  }
}

impl<K: Into<Key>> FromIterator<(K, Value)> for ValueMap {
  fn from_iter<T: IntoIterator<Item = (K, Value)>>(iter: T) -> Self {
    ValueMap(Held::new(Fields::from_iter(iter)))
  }
}

impl IntoIterator for ValueMap {
  type Item = (Key, Value);
  type IntoIter = std::vec::IntoIter<(Key, Value)>;
  fn into_iter(self) -> Self::IntoIter {
    self.into_fields().into_iter()
  }
}

impl<'a> IntoIterator for &'a ValueMap {
  type Item = (&'a Key, &'a Value);
  type IntoIter = <&'a Fields as IntoIterator>::IntoIter;
  fn into_iter(self) -> Self::IntoIter {
    (&**self).into_iter()
  }
}

impl<K: Into<Key>> Extend<(K, Value)> for ValueMap {
  fn extend<T: IntoIterator<Item = (K, Value)>>(&mut self, iter: T) {
    self.0.make_mut().extend(iter);
  }
}

/// The tags of a `Value::Variant` standing for a JavaScript value the model has no arm for. The browser makes the real type from the tag; the server reads the payload as the plain value the type lowers to: a `Map` as an array of `[key, value]` pairs, a `Set` as an array, a `Date` as milliseconds since the epoch, a `URL` as its href and a `BigInt` as an integer.
pub mod typed {
  pub const DATE: &str = "$date";
  pub const MAP: &str = "$map";
  pub const SET: &str = "$set";
  pub const BIGINT: &str = "$bigint";
  pub const URL: &str = "$url";
  /// A Temporal value: its type's name and its canonical string, `["PlainDate", "2024-02-29"]`.
  pub const TEMPORAL: &str = "$temporal";
  /// A symbol from `Symbol.for(key)`: its key, so two of the same key are the same symbol.
  pub const SYMBOL: &str = "$symbol";
  /// `JSON.rawJSON(text)`: the JSON text `JSON.stringify` writes as it is.
  pub const RAW_JSON: &str = "$rawJSON";
  /// A built-in constructor's `prototype`, its payload the constructor's name, so `Object.getPrototypeOf(x) === Array.prototype` compares two of them.
  pub const PROTOTYPE: &str = "$prototype";
  /// The field marking the object that stands for a promise `Promise.withResolvers()` made, which holds its `$state` and its `value` or `reason`.
  pub const DEFERRED: &str = "$deferred";
  /// `new Number(n)`, `new Boolean(b)` or `new String(s)`: a wrapper object, its payload the primitive it wraps, which JSON writes.
  pub const WRAPPER: &str = "$wrapper";
  /// The field holding a regular expression's pattern in the object that stands for it, beside its flags and its `lastIndex`.
  pub const REGEX: &str = "$regex";
  /// A function value: its payload indexes the closure table of the run that made it.
  pub const FUNCTION: &str = "$fn";
  /// `new Request(url, init)`: its payload an object holding the request's `url`, `method` and the other fields the Fetch standard gives it.
  pub const REQUEST: &str = "$request";

  pub const ALL: [&str; 6] = [DATE, MAP, SET, BIGINT, URL, TEMPORAL];

  /// The description of the symbol held under `key`: `Symbol.iterator` for `@@iterator`, the key `Symbol.for` registered for `@@for:key`, the description `Symbol(description)` gave for `@@#n:description`, `None` for `@@#n`.
  pub fn symbol_description(key: &str) -> Option<String> {
    if let Some(registered) = key.strip_prefix("@@for:") {
      return Some(registered.to_owned());
    }
    if let Some(unique) = key.strip_prefix("@@#") {
      return unique.split_once(':').map(|(_, description)| description.to_owned());
    }
    Some(match key.strip_prefix("@@") {
      Some(name) => format!("Symbol.{name}"),
      None => key.to_owned(),
    })
  }

  /// Whether a field name is the key a symbol is held under, `@@iterator`, which `Object.keys` and JSON leave out.
  pub fn symbol_key(key: &str) -> bool {
    key.starts_with("@@")
  }

  /// The milliseconds a `Date` value holds, `None` for any other value.
  pub fn date_ms(value: &super::Value) -> Option<f64> {
    match value {
      super::Value::Variant { tag, payload: Some(ms) } if tag == DATE => match **ms {
        super::Value::F64(ms) => Some(ms),
        super::Value::Int(ms) => Some(ms as f64),
        _ => None,
      },
      _ => None,
    }
  }

  /// A Temporal value's type name and canonical string, the text `toString` and `toJSON` give.
  pub fn temporal(value: &super::Value) -> Option<(&str, &str)> {
    let super::Value::Variant { tag, payload: Some(payload) } = value else { return None };
    let super::Value::Seq(parts) = &**payload else { return None };
    match (tag.as_str(), parts.get(0), parts.get(1)) {
      (TEMPORAL, Some(super::Value::Str(kind)), Some(super::Value::Str(text))) => Some((kind.as_str(), text.as_str())),
      _ => None,
    }
  }

  /// A Temporal value of type `kind` whose canonical string is `text`.
  pub fn temporal_value(kind: &str, text: &str) -> super::Value {
    super::Value::Variant { tag: TEMPORAL.to_owned(), payload: Some(Box::new(super::Value::seq(vec![super::Value::str(kind), super::Value::str(text)]))) }
  }
}

/// Equal values fingerprint equal and unequal values fingerprint apart. A
/// float compares by its canonical bits, so every NaN is one value and `0.0`
/// is not `-0.0`. An unsigned value that fits `i128` equals its signed form
/// however it was constructed.
#[derive(Debug, Clone)]
pub enum Value {
  Null,
  /// JavaScript's `undefined`: what a missing field, a missing argument or a function with no `return` reads as. A field holding it is left out where the value is written as JSON.
  Undefined,
  Bool(bool),
  /// Integers up to i128; unsigned values that fit are normalized here.
  Int(i128),
  /// Only for magnitudes above i128::MAX. Constructors enforce the normalization.
  UInt(u128),
  F32(f32),
  F64(f64),
  Str(ValueStr),
  Bytes(Vec<u8>),
  TypedArray(TypedArray),
  Seq(ValueSeq),
  Map(ValueMap),
  Variant { tag: String, payload: Option<Box<Value>> },
  Ref { kind: RefKind, id: String },
}

impl Value {
  /// Whether this holds an object or an array nested more than `limit` levels down.
  pub fn nested_past(&self, limit: usize) -> bool {
    let mut waiting = vec![(self, 0)];
    while let Some((value, depth)) = waiting.pop() {
      let inner: &mut dyn Iterator<Item = &Value> = match value {
        Value::Map(map) => &mut map.values(),
        Value::Seq(items) => &mut items.iter(),
        _ => continue,
      };
      if depth == limit {
        return true;
      }
      for held in inner {
        waiting.push((held, depth + 1));
      }
    }
    false
  }
}

impl PartialEq for Value {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Value::Null, Value::Null) | (Value::Undefined, Value::Undefined) => true,
      (Value::Bool(a), Value::Bool(b)) => a == b,
      (Value::Int(a), Value::Int(b)) => a == b,
      (Value::UInt(a), Value::UInt(b)) => a == b,
      (Value::Int(a), Value::UInt(b)) | (Value::UInt(b), Value::Int(a)) => i128::try_from(*b).is_ok_and(|b| b == *a),
      (Value::F32(a), Value::F32(b)) => canonical_f32_bits(*a) == canonical_f32_bits(*b),
      (Value::F64(a), Value::F64(b)) => canonical_f64_bits(*a) == canonical_f64_bits(*b),
      (Value::Str(a), Value::Str(b)) => a == b,
      (Value::Bytes(a), Value::Bytes(b)) => a == b,
      (Value::TypedArray(a), Value::TypedArray(b)) => a == b,
      (Value::Seq(a), Value::Seq(b)) => a == b,
      (Value::Map(a), Value::Map(b)) => a == b,
      (Value::Variant { tag: a, payload: p }, Value::Variant { tag: b, payload: q }) => a == b && p == q,
      (Value::Ref { kind: a, id: i }, Value::Ref { kind: b, id: j }) => a == b && i == j,
      _ => false,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
  Action,
  Module,
}

#[derive(Debug, Clone)]
pub enum TypedArray {
  I8(Vec<i8>),
  U8(Vec<u8>),
  I16(Vec<i16>),
  U16(Vec<u16>),
  I32(Vec<i32>),
  U32(Vec<u32>),
  I64(Vec<i64>),
  U64(Vec<u64>),
  F32(Vec<f32>),
  F64(Vec<f64>),
}

impl PartialEq for TypedArray {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (TypedArray::I8(a), TypedArray::I8(b)) => a == b,
      (TypedArray::U8(a), TypedArray::U8(b)) => a == b,
      (TypedArray::I16(a), TypedArray::I16(b)) => a == b,
      (TypedArray::U16(a), TypedArray::U16(b)) => a == b,
      (TypedArray::I32(a), TypedArray::I32(b)) => a == b,
      (TypedArray::U32(a), TypedArray::U32(b)) => a == b,
      (TypedArray::I64(a), TypedArray::I64(b)) => a == b,
      (TypedArray::U64(a), TypedArray::U64(b)) => a == b,
      (TypedArray::F32(a), TypedArray::F32(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| canonical_f32_bits(*x) == canonical_f32_bits(*y)),
      (TypedArray::F64(a), TypedArray::F64(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| canonical_f64_bits(*x) == canonical_f64_bits(*y)),
      _ => false,
    }
  }
}

impl Value {
  pub fn int(v: impl Into<i128>) -> Self {
    Value::Int(v.into())
  }

  pub fn uint(v: u128) -> Self {
    match i128::try_from(v) {
      Ok(i) => Value::Int(i),
      Err(_) => Value::UInt(v),
    }
  }

  pub fn str(v: impl Into<ValueStr>) -> Self {
    Value::Str(v.into())
  }

  pub fn seq(items: impl Into<ValueSeq>) -> Self {
    Value::Seq(items.into())
  }

  pub fn action_ref(id: impl Into<String>) -> Self {
    Value::Ref { kind: RefKind::Action, id: id.into() }
  }
}

impl From<bool> for Value {
  fn from(v: bool) -> Self {
    Value::Bool(v)
  }
}

impl From<&str> for Value {
  fn from(v: &str) -> Self {
    Value::Str(ValueStr::from(v))
  }
}

impl From<String> for Value {
  fn from(v: String) -> Self {
    Value::Str(ValueStr::from(v))
  }
}

impl From<i64> for Value {
  fn from(v: i64) -> Self {
    Value::Int(v.into())
  }
}

impl From<u64> for Value {
  fn from(v: u64) -> Self {
    Value::Int(v.into())
  }
}

impl From<f64> for Value {
  fn from(v: f64) -> Self {
    Value::F64(v)
  }
}

impl From<Key> for ValueStr {
  fn from(v: Key) -> Self {
    ValueStr(v)
  }
}

impl From<&Key> for ValueStr {
  fn from(v: &Key) -> Self {
    ValueStr(v.clone())
  }
}
