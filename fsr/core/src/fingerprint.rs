use xxhash_rust::xxh3::Xxh3;

use crate::node::Node;
use crate::plan::PlanNode;
use crate::value::{RefKind, TypedArray, Value};

/// Canonical content hash. Equal values hash equal regardless of construction
/// history: map entries are hashed in key order, every NaN collapses to one bit
/// pattern and an unsigned value that fits i128 hashes as its signed form.
pub trait Fingerprint {
  fn write_canonical(&self, h: &mut Xxh3);

  fn fingerprint(&self) -> u64 {
    let mut h = Xxh3::new();
    self.write_canonical(&mut h);
    h.digest()
  }
}

fn write_len(h: &mut Xxh3, len: usize) {
  h.update(&(len as u64).to_le_bytes());
}

fn write_str(h: &mut Xxh3, s: &str) {
  write_len(h, s.len());
  h.update(s.as_bytes());
}

fn canonical_f64_bits(v: f64) -> u64 {
  if v.is_nan() { 0x7ff8_0000_0000_0000 } else { v.to_bits() }
}

fn canonical_f32_bits(v: f32) -> u32 {
  if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() }
}

fn write_map(h: &mut Xxh3, map: &crate::value::ValueMap) {
  write_len(h, map.len());
  let mut keys: Vec<&String> = map.keys().collect();
  keys.sort_unstable();
  for key in keys {
    write_str(h, key);
    map[key.as_str()].write_canonical(h);
  }
}

impl Fingerprint for Value {
  fn write_canonical(&self, h: &mut Xxh3) {
    match self {
      Value::Null => h.update(&[0]),
      Value::Bool(v) => {
        h.update(&[1]);
        h.update(&[*v as u8]);
      }
      Value::Int(v) => {
        h.update(&[2]);
        h.update(&v.to_le_bytes());
      }
      Value::UInt(v) => match i128::try_from(*v) {
        Ok(i) => {
          h.update(&[2]);
          h.update(&i.to_le_bytes());
        }
        Err(_) => {
          h.update(&[3]);
          h.update(&v.to_le_bytes());
        }
      },
      Value::F32(v) => {
        h.update(&[4]);
        h.update(&canonical_f32_bits(*v).to_le_bytes());
      }
      Value::F64(v) => {
        h.update(&[5]);
        h.update(&canonical_f64_bits(*v).to_le_bytes());
      }
      Value::Str(v) => {
        h.update(&[6]);
        write_str(h, v);
      }
      Value::Bytes(v) => {
        h.update(&[7]);
        write_len(h, v.len());
        h.update(v);
      }
      Value::TypedArray(v) => {
        h.update(&[8]);
        v.write_canonical(h);
      }
      Value::Seq(v) => {
        h.update(&[9]);
        write_len(h, v.len());
        for item in v {
          item.write_canonical(h);
        }
      }
      Value::Map(v) => {
        h.update(&[10]);
        write_map(h, v);
      }
      Value::Variant { tag, payload } => {
        h.update(&[11]);
        write_str(h, tag);
        match payload {
          None => h.update(&[0]),
          Some(p) => {
            h.update(&[1]);
            p.write_canonical(h);
          }
        }
      }
      Value::Ref { kind, id } => {
        h.update(&[12]);
        h.update(&[match kind {
          RefKind::Action => 0,
          RefKind::Module => 1,
        }]);
        write_str(h, id);
      }
    }
  }
}

impl Fingerprint for crate::value::ValueMap {
  fn write_canonical(&self, h: &mut Xxh3) {
    write_map(h, self);
  }
}

impl Fingerprint for TypedArray {
  fn write_canonical(&self, h: &mut Xxh3) {
    macro_rules! arm {
      ($tag:expr, $items:expr) => {{
        h.update(&[$tag]);
        write_len(h, $items.len());
        for item in $items {
          h.update(&item.to_le_bytes());
        }
      }};
    }
    match self {
      TypedArray::I8(v) => arm!(0, v),
      TypedArray::U8(v) => arm!(1, v),
      TypedArray::I16(v) => arm!(2, v),
      TypedArray::U16(v) => arm!(3, v),
      TypedArray::I32(v) => arm!(4, v),
      TypedArray::U32(v) => arm!(5, v),
      TypedArray::I64(v) => arm!(6, v),
      TypedArray::U64(v) => arm!(7, v),
      TypedArray::F32(v) => {
        h.update(&[8]);
        write_len(h, v.len());
        for item in v {
          h.update(&canonical_f32_bits(*item).to_le_bytes());
        }
      }
      TypedArray::F64(v) => {
        h.update(&[9]);
        write_len(h, v.len());
        for item in v {
          h.update(&canonical_f64_bits(*item).to_le_bytes());
        }
      }
    }
  }
}

impl Fingerprint for Node {
  fn write_canonical(&self, h: &mut Xxh3) {
    match self {
      Node::Text(v) => {
        h.update(&[0]);
        write_str(h, v);
      }
      Node::Raw(v) => {
        h.update(&[1]);
        write_str(h, &v.0);
      }
      Node::Seq(v) => {
        h.update(&[2]);
        write_len(h, v.len());
        for item in v {
          item.write_canonical(h);
        }
      }
      Node::Client { module, props, children, ssr } => {
        h.update(&[3]);
        write_str(h, &module.path);
        write_str(h, &module.export);
        write_map(h, props);
        write_len(h, children.len());
        for child in children {
          child.write_canonical(h);
        }
        match ssr {
          None => h.update(&[0]),
          Some(n) => {
            h.update(&[1]);
            n.write_canonical(h);
          }
        }
      }
      Node::Pending { slot, fallback } => {
        h.update(&[4]);
        h.update(&slot.0.to_le_bytes());
        fallback.write_canonical(h);
      }
    }
  }
}

impl Fingerprint for PlanNode {
  fn write_canonical(&self, h: &mut Xxh3) {
    h.update(&self.id.0.to_le_bytes());
    write_str(h, &self.module.path);
    write_str(h, &self.module.export);
    match &self.data_source {
      None => h.update(&[0]),
      Some(ds) => {
        h.update(&[1]);
        write_str(h, &ds.0);
      }
    }
    h.update(&[self.deferred as u8]);
    match &self.fallback {
      None => h.update(&[0]),
      Some(m) => {
        h.update(&[1]);
        write_str(h, &m.path);
        write_str(h, &m.export);
      }
    }
    match &self.cache_key {
      None => h.update(&[0]),
      Some(k) => {
        h.update(&[1]);
        write_str(h, &k.0);
      }
    }
    write_len(h, self.children.len());
    for (slot, child) in &self.children {
      write_str(h, &slot.0);
      child.write_canonical(h);
    }
  }
}
