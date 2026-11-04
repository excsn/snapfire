use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;

#[cfg(feature = "http")]
mod transport;
#[cfg(feature = "http")]
pub use transport::HttpTransport;

/// How one contract method reaches the wire. Defaults to
/// `POST {base}/{service}/{method}` with the arguments as a JSON body, which
/// is what a plain gateway wants; `route` overrides per method.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Route {
  pub method: String,
  pub path: String,
}

impl Route {
  pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
    Self { method: method.into(), path: path.into() }
  }

  pub fn get(path: impl Into<String>) -> Self {
    Self::new("GET", path)
  }

  pub fn post(path: impl Into<String>) -> Self {
    Self::new("POST", path)
  }

  /// `{name}` segments take the argument of that name, which is then not sent
  /// in the body. An argument the call does not carry is refused: substituting
  /// nothing would send a request with an empty path segment and leave the
  /// argument in the body, which no server can answer as the caller meant.
  #[cfg_attr(not(feature = "http"), allow(dead_code))]
  pub(crate) fn render(&self, args: &ValueMap) -> Result<(String, Vec<String>), String> {
    let mut path = String::new();
    let mut consumed = Vec::new();
    let mut rest = self.path.as_str();
    while let Some(open) = rest.find('{') {
      let Some(close) = rest[open..].find('}') else { break };
      let name = &rest[open + 1..open + close];
      path.push_str(&rest[..open]);
      let Some(value) = args.get(name) else {
        return Err(format!("the route `{}` names `{name}`, which the call does not carry", self.path));
      };
      path.push_str(&scalar_to_path(value));
      consumed.push(name.to_owned());
      rest = &rest[open + close + 1..];
    }
    path.push_str(rest);
    Ok((path, consumed))
  }
}

#[cfg_attr(not(feature = "http"), allow(dead_code))]
fn scalar_to_path(value: &Value) -> String {
  match value {
    Value::Str(s) => s.to_string(),
    Value::Int(v) => v.to_string(),
    Value::UInt(v) => v.to_string(),
    Value::Bool(b) => b.to_string(),
    Value::F32(v) => v.to_string(),
    Value::F64(v) => v.to_string(),
    other => format!("{other:?}"),
  }
}

/// `text` with every `\u` escape of an unpaired surrogate written as `�`, which is what a browser displays for one; JSON allows them and a Rust string cannot hold them.
#[cfg_attr(not(any(feature = "http", feature = "graphql")), allow(dead_code))]
pub(crate) fn replace_lone_surrogates(text: &str) -> std::borrow::Cow<'_, str> {
  if !text.contains("\\u") {
    return std::borrow::Cow::Borrowed(text);
  }
  let bytes = text.as_bytes();
  let unit = |at: usize| -> Option<u16> {
    let escape = bytes.get(at..at + 6)?;
    if escape[0] != b'\\' || escape[1] != b'u' {
      return None;
    }
    u16::from_str_radix(std::str::from_utf8(&escape[2..]).ok()?, 16).ok()
  };
  let mut out = String::with_capacity(text.len());
  let (mut copied, mut i, mut in_string) = (0, 0, false);
  while i < bytes.len() {
    match bytes[i] {
      b'"' => in_string = !in_string,
      b'\\' if in_string => {
        match unit(i) {
          Some(0xD800..=0xDBFF) if matches!(unit(i + 6), Some(0xDC00..=0xDFFF)) => i += 12,
          Some(0xD800..=0xDFFF) => {
            out.push_str(&text[copied..i]);
            out.push_str("\\uFFFD");
            i += 6;
            copied = i;
          }
          _ => i += 2,
        }
        continue;
      }
      _ => {}
    }
    i += 1;
  }
  if copied == 0 {
    return std::borrow::Cow::Borrowed(text);
  }
  out.push_str(&text[copied..]);
  std::borrow::Cow::Owned(out)
}

pub fn kind_for_status(status: u16) -> FailureKind {
  FailureKind::from_status(status)
}
