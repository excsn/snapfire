//! What React 18.3 writes where it differs from 19. Its stable build leaves
//! custom element property support off, so every prop of a custom element
//! is coerced to a string.

use snapfire_fsr_core::{TypedArray, Value};

use super::{write_attr, BOOLEAN};
use crate::interp::{scalar_str, Fail};

/// A custom element's attribute, coerced the way `'' + value` coerces it:
/// `true` is `"true"`, `false` is `"false"`, an array or a typed array is its
/// items joined by commas and an object is `[object Object]`. The one place
/// this departs from 18 is `className`, which the renderer writes as `class`
/// where 18 wrote `className` and 19 fixed it.
pub(super) fn custom_attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  let mut text = String::new();
  coerce(value, &mut text)?;
  write_attr(name, &text, out);
  Ok(())
}

fn coerce(value: &Value, out: &mut String) -> Result<(), Fail> {
  match value {
    Value::Seq(items) => {
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        if !matches!(item, Value::Null) {
          coerce(item, out)?;
        }
      }
    }
    Value::Bytes(bytes) => coerce(&Value::seq(bytes.iter().map(|b| Value::UInt((*b).into())).collect::<Vec<Value>>()), out)?,
    Value::TypedArray(array) => coerce(&Value::seq(elements(array)), out)?,
    Value::Map(_) | Value::Variant { .. } | Value::Ref { .. } => out.push_str("[object Object]"),
    scalar => out.push_str(&scalar_str(scalar)?),
  }
  Ok(())
}

fn elements(array: &TypedArray) -> Vec<Value> {
  match array {
    TypedArray::I8(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::U8(v) => v.iter().map(|n| Value::UInt((*n).into())).collect(),
    TypedArray::I16(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::U16(v) => v.iter().map(|n| Value::UInt((*n).into())).collect(),
    TypedArray::I32(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::U32(v) => v.iter().map(|n| Value::UInt((*n).into())).collect(),
    TypedArray::I64(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::U64(v) => v.iter().map(|n| Value::UInt((*n).into())).collect(),
    TypedArray::F32(v) => v.iter().map(|f| Value::F32(*f)).collect(),
    TypedArray::F64(v) => v.iter().map(|f| Value::F64(*f)).collect(),
  }
}

pub(super) fn is_boolean(name: &str) -> bool {
  BOOLEAN.contains(&name)
}

/// React 18 does not know `inert` and drops a boolean on an attribute it does not know.
pub(super) fn drops_true(name: &str) -> bool {
  name == "inert"
}
