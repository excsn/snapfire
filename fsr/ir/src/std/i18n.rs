//! `i18n.t`, which `t` from the standard library lowers to: a key looked up
//! in the ambient locale's catalog, a plural form chosen by `count`, and
//! `{name}` placeholders filled from the arguments.

use snapfire_fsr_core::Value;

use crate::ext::{text, Ambient, Extensions, Reach};
use crate::interp::{stringify, Fail};

pub fn register(extensions: &mut Extensions) {
  extensions.register("i18n.t", Reach::Render, t);
}

/// The forms `t` tries for `key` under `count`, most specific first.
pub fn candidates(key: &str, category: Option<&str>) -> Vec<String> {
  match category {
    Some(category) => vec![format!("{key}.{category}"), format!("{key}.other"), key.to_owned()],
    None => vec![key.to_owned()],
  }
}

/// `text` with every `{name}` replaced by `args[name]` stringified; a
/// placeholder no argument fills stays as written.
pub fn interpolate(text: &str, args: Option<&snapfire_fsr_core::ValueMap>) -> Result<String, Fail> {
  let Some(args) = args else { return Ok(text.to_owned()) };
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find('{') {
    out.push_str(&rest[..open]);
    let after = &rest[open + 1..];
    match after.find('}') {
      Some(close) if !after[..close].is_empty() && after[..close].chars().all(|c| c.is_alphanumeric() || c == '_') => {
        let name = &after[..close];
        match args.get(name) {
          Some(value) if !matches!(value, Value::Map(_) | Value::Seq(_)) => out.push_str(&stringify(value)?),
          _ => {
            out.push('{');
            out.push_str(name);
            out.push('}');
          }
        }
        rest = &after[close + 1..];
      }
      _ => {
        out.push('{');
        rest = after;
      }
    }
  }
  out.push_str(rest);
  Ok(out)
}

fn t(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "i18n.t";
  let key = text(what, args, 0)?;
  let options = match args.get(1) {
    Some(Value::Map(map)) => Some(map),
    None | Some(Value::Null) => None,
    Some(other) => return Err(crate::interp::type_error(what, "an arguments object", other)),
  };
  let category = match options.and_then(|o| o.get("count")) {
    Some(count) => Some(super::intl::category(ambient, count)?),
    None => None,
  };
  let found = ambient.catalogs.as_ref().and_then(|catalogs| candidates(key, category.as_deref()).into_iter().find_map(|k| catalogs.lookup(&ambient.locale, &k).map(str::to_owned)));
  let text = found.unwrap_or_else(|| key.to_owned());
  Ok(Value::Str(interpolate(&text, options)?))
}
