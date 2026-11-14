//! A map from text keys to text values held in one run of bytes and read in place.
//!
//! - bytes 0 to 8: `SFI18N`, a zero byte and the format's version.
//! - bytes 8 to 12: the number of slots, little-endian.
//! - bytes 12 to 16: the length of the records in bytes, little-endian.
//! - the slots, four bytes each, little-endian: zero for an empty slot, else one more than the offset of a record.
//! - the records, each a key's length in one byte, a value's length in two bytes, little-endian, the key and the value.
//!
//! A key is hashed a tab-separated part at a time, eight bytes at a go. Its slot is the hash scaled to the number of slots, which is twice the number of records. A key that finds its slot taken sits in the next free one.

const MAGIC: &[u8; 8] = b"SFI18N\x00\x01";
const HEADER: usize = 16;

/// A table read in place.
#[derive(Clone, Copy)]
pub(crate) struct Table {
  slots: &'static [u8],
  records: &'static [u8],
}

const SEED: u64 = 0x517c_c1b7_2722_0a95;

fn mixed(hash: u64, word: u64) -> u64 {
  (hash.rotate_left(5) ^ word).wrapping_mul(SEED)
}

fn hash<'a>(parts: impl Iterator<Item = &'a str>) -> u64 {
  let mut hash = 0;
  for part in parts {
    let mut chunks = part.as_bytes().chunks_exact(8);
    for chunk in &mut chunks {
      hash = mixed(hash, u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8])));
    }
    let mut last = [0u8; 8];
    last[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
    hash = mixed(mixed(hash, u64::from_le_bytes(last)), part.len() as u64);
  }
  hash
}

/// The slot `hash` starts at among `count`.
fn slot_of(hash: u64, count: usize) -> usize {
  ((u128::from(hash) * count as u128) >> 64) as usize
}

/// Whether `key` is `parts` joined by tabs.
fn is_joined(key: &[u8], parts: &[&str]) -> bool {
  let mut rest = key;
  for (i, part) in parts.iter().enumerate() {
    if i > 0 {
      match rest.split_first() {
        Some((b'\t', after)) => rest = after,
        _ => return false,
      }
    }
    match rest.strip_prefix(part.as_bytes()) {
      Some(after) => rest = after,
      None => return false,
    }
  }
  rest.is_empty()
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
  Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
}

impl Table {
  /// The table `bytes` hold, `None` for bytes that are not one of this version.
  pub(crate) fn open(bytes: &'static [u8]) -> Option<Table> {
    if bytes.get(..MAGIC.len())? != MAGIC {
      return None;
    }
    let slots = u32_at(bytes, 8)?.checked_mul(4)?;
    let records = u32_at(bytes, 12)?;
    if slots == 0 || bytes.len() != HEADER.checked_add(slots)?.checked_add(records)? {
      return None;
    }
    Some(Table { slots: &bytes[HEADER..HEADER + slots], records: &bytes[HEADER + slots..] })
  }

  /// The key and the value of the record at `at` and the offset of the record after it.
  fn record(&self, at: usize) -> Option<(&'static [u8], &'static [u8], usize)> {
    let key_len = usize::from(*self.records.get(at)?);
    let value_len = usize::from(u16::from_le_bytes(self.records.get(at + 1..at + 3)?.try_into().ok()?));
    let key = self.records.get(at + 3..at + 3 + key_len)?;
    let value = self.records.get(at + 3 + key_len..at + 3 + key_len + value_len)?;
    Some((key, value, at + 3 + key_len + value_len))
  }

  /// The value under the key `parts` joined by tabs.
  pub(crate) fn get(&self, parts: &[&str]) -> Option<&'static str> {
    let count = self.slots.len() / 4;
    let mut slot = slot_of(hash(parts.iter().copied()), count);
    for _ in 0..count {
      let at = u32_at(self.slots, slot * 4)?.checked_sub(1)?;
      let (key, value, _) = self.record(at)?;
      if is_joined(key, parts) {
        return std::str::from_utf8(value).ok();
      }
      slot = if slot + 1 == count { 0 } else { slot + 1 };
    }
    None
  }

  /// Whether `other` reads the same bytes.
  pub(crate) fn is(&self, other: &Table) -> bool {
    std::ptr::eq(self.records, other.records)
  }

  /// Every key and its value, in the order they were written.
  pub(crate) fn records(&self) -> impl Iterator<Item = (&'static str, &'static str)> + use<> {
    let table = *self;
    let mut at = 0;
    std::iter::from_fn(move || {
      let (key, value, next) = table.record(at)?;
      at = next;
      Some((std::str::from_utf8(key).ok()?, std::str::from_utf8(value).ok()?))
    })
  }
}

/// The bytes of a table holding `records`. A key written twice, a key over 255 bytes and a value over 65,535 bytes are refused.
pub(crate) fn write<'a>(records: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Vec<u8>, String> {
  let records: Vec<(&str, &str)> = records.into_iter().collect();
  let count = (records.len() * 2).max(2);
  let mut slots = vec![0u32; count];
  let mut heap: Vec<u8> = Vec::new();
  for (key, value) in &records {
    let key_len = u8::try_from(key.len()).map_err(|_| format!("the key `{key}` is over 255 bytes"))?;
    let value_len = u16::try_from(value.len()).map_err(|_| format!("the value of `{key}` is over 65,535 bytes"))?;
    let at = u32::try_from(heap.len() + 1).map_err(|_| "the records are over 4 GiB".to_owned())?;
    let mut slot = slot_of(hash(key.split('\t')), count);
    while slots[slot] != 0 {
      let taken = slots[slot] as usize - 1;
      let taken_len = usize::from(heap[taken]);
      if &heap[taken + 3..taken + 3 + taken_len] == key.as_bytes() {
        return Err(format!("the key `{key}` is written twice"));
      }
      slot = if slot + 1 == count { 0 } else { slot + 1 };
    }
    slots[slot] = at;
    heap.push(key_len);
    heap.extend_from_slice(&value_len.to_le_bytes());
    heap.extend_from_slice(key.as_bytes());
    heap.extend_from_slice(value.as_bytes());
  }
  let mut out = Vec::with_capacity(HEADER + count * 4 + heap.len());
  out.extend_from_slice(MAGIC);
  out.extend_from_slice(&(count as u32).to_le_bytes());
  out.extend_from_slice(&(heap.len() as u32).to_le_bytes());
  for slot in &slots {
    out.extend_from_slice(&slot.to_le_bytes());
  }
  out.extend_from_slice(&heap);
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::{write, Table};

  fn leaked(records: &[(&str, &str)]) -> Table {
    Table::open(Box::leak(write(records.iter().copied()).unwrap().into_boxed_slice())).unwrap()
  }

  #[test]
  fn a_key_reads_the_value_written_under_it() {
    let table = leaked(&[("zN\ten\tAcre", "Acre Standard Time\tAcre Summer Time"), ("zV", "48"), ("zT\tfi", ".")]);
    assert_eq!(table.get(&["zN", "en", "Acre"]), Some("Acre Standard Time\tAcre Summer Time"));
    assert_eq!(table.get(&["zV"]), Some("48"));
    assert_eq!(table.get(&["zT", "fi"]), Some("."));
  }

  #[test]
  fn a_key_not_written_reads_nothing() {
    let table = leaked(&[("zN\ten\tAcre", "a"), ("zT\tfi", ".")]);
    assert_eq!(table.get(&["zN", "en"]), None);
    assert_eq!(table.get(&["zN", "en", "Acre", ""]), None);
    assert_eq!(table.get(&["zN", "enAcre"]), None);
    assert_eq!(table.get(&["zT", "f"]), None);
    assert_eq!(table.get(&[]), None);
  }

  #[test]
  fn many_keys_each_read_their_own_value() {
    let keys: Vec<String> = (0..5000).map(|i| format!("k\t{i}")).collect();
    let values: Vec<String> = (0..5000).map(|i| format!("v{i}")).collect();
    let records: Vec<(&str, &str)> = keys.iter().map(String::as_str).zip(values.iter().map(String::as_str)).collect();
    let table = leaked(&records);
    for i in 0..5000 {
      assert_eq!(table.get(&["k", &i.to_string()]), Some(values[i].as_str()));
    }
    assert_eq!(table.records().count(), 5000);
    assert_eq!(table.records().next(), Some(("k\t0", "v0")));
  }

  #[test]
  fn a_table_with_no_records_reads_nothing() {
    assert_eq!(leaked(&[]).get(&["a"]), None);
  }

  #[test]
  fn a_key_written_twice_is_refused() {
    assert!(write([("a", "1"), ("a", "2")]).is_err());
  }

  #[test]
  fn bytes_that_are_not_a_table_are_refused() {
    assert!(Table::open(b"").is_none());
    assert!(Table::open(b"SFI18N\x00\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00").is_none());
    let cut: &'static [u8] = Box::leak(write([("a", "1")]).unwrap().into_boxed_slice());
    assert!(Table::open(&cut[..cut.len() - 1]).is_none());
  }

  #[test]
  fn a_table_whose_slots_point_outside_it_reads_nothing() {
    let mut bytes = write([("a", "1")]).unwrap();
    for byte in &mut bytes[16..24] {
      *byte = 0xff;
    }
    let table = Table::open(Box::leak(bytes.into_boxed_slice())).unwrap();
    assert_eq!(table.get(&["a"]), None);
  }
}
