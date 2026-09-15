//! What React 19 writes where it differs from 18. The rules below hold from
//! 19.0 through 19.3.

use std::borrow::Cow;

use snapfire_fsr_core::Value;

use super::{write_attr, BOOLEAN};
use crate::interp::{scalar_str, Fail};

/// A custom element's attribute: `true` writes `name=""`, `false` writes
/// nothing and an object or an array writes nothing, since React 19 sets it
/// as a property once the element is in the browser.
pub(super) fn custom_attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  match value {
    Value::Bool(true) => write_attr(name, "", out),
    Value::Str(_) | Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) => write_attr(name, &scalar_str(value)?, out),
    _ => {}
  }
  Ok(())
}

pub(super) fn is_boolean(name: &str) -> bool {
  name == "inert" || BOOLEAN.contains(&name)
}

/// An empty `src` anywhere or an empty `href` on anything but `<a>`. On `<a>`
/// an empty `href` links to the page itself.
pub(super) fn drops_empty(tag: &str, name: &str) -> bool {
  name == "src" || (name == "href" && tag != "a")
}

pub(super) fn may_hoist(tag: &str) -> bool {
  matches!(tag, "title" | "meta" | "link")
}

/// A `<title>` or a `<meta>` moves unless it carries `itemProp`. A `<link>`
/// moves when it has a `rel`, a non-empty `href` and no load or error
/// handler; a stylesheet moves only with a `precedence` and no `disabled`.
pub(super) fn hoists(tag: &str, attrs: &[(Cow<'_, str>, Value)]) -> bool {
  let get = |name: &str| attrs.iter().find(|(n, v)| n.as_ref() == name && !matches!(v, Value::Null)).map(|(_, v)| v);
  if get("itemProp").is_some() {
    return false;
  }
  match tag {
    "title" | "meta" => true,
    "link" => {
      let (Some(Value::Str(rel)), Some(Value::Str(href))) = (get("rel"), get("href")) else {
        return false;
      };
      let handled = get("$on:load").is_some() || get("$on:error").is_some();
      if href.is_empty() || handled {
        return false;
      }
      rel.as_str() != "stylesheet" || (matches!(get("precedence"), Some(Value::Str(_))) && get("disabled").is_none())
    }
    _ => false,
  }
}
