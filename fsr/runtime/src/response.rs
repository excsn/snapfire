//! A response a body answers in place of data: what `new Response(...)`, `Response.json`, `Response.redirect` and `redirect()` make.

use snapfire_fsr_core::{TypedArray, Value, ValueMap};

/// The tag of the `Value::Variant` that holds an [`HttpResponse`]. Its payload is an object of `status`, `headers` and `body`: the headers an array of `[name, value]` pairs so a header may repeat; the body a string or bytes.
pub const RESPONSE_TAG: &str = "$response";

/// The statuses `Response.redirect` takes, as the Fetch standard lists them.
pub const REDIRECT_STATUSES: [u16; 5] = [301, 302, 303, 307, 308];

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
  pub status: u16,
  /// In the order given, names as written.
  pub headers: Vec<(String, String)>,
  pub body: Vec<u8>,
  /// Which route response a middleware's `next` answered this stands for, whose body the host holds.
  pub downstream: Option<usize>,
}

impl HttpResponse {
  pub fn new(status: u16) -> Self {
    Self { status, headers: Vec::new(), body: Vec::new(), downstream: None }
  }

  /// The value of the header `name`, matched without regard to case.
  pub fn header(&self, name: &str) -> Option<&str> {
    self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
  }

  /// Sets `name` unless a header of that name is already there.
  pub fn header_default(&mut self, name: &str, value: &str) {
    if self.header(name).is_none() {
      self.headers.push((name.to_owned(), value.to_owned()));
    }
  }

  /// Where a redirect sends the caller: the `Location` of a 3xx response.
  pub fn location(&self) -> Option<&str> {
    (300..400).contains(&self.status).then(|| self.header("location")).flatten()
  }

  pub fn to_value(&self) -> Value {
    let headers: Vec<Value> = self.headers.iter().map(|(name, value)| Value::seq(vec![Value::str(name.clone()), Value::str(value.clone())])).collect();
    let mut fields = ValueMap::default();
    fields.insert("status".to_owned(), Value::Int(i128::from(self.status)));
    fields.insert("headers".to_owned(), Value::seq(headers));
    fields.insert("body".to_owned(), Value::Bytes(self.body.clone()));
    if let Some(at) = self.downstream {
      fields.insert("downstream".to_owned(), Value::Int(at as i128));
    }
    Value::Variant { tag: RESPONSE_TAG.to_owned(), payload: Some(Box::new(Value::Map(fields))) }
  }

  /// The response `value` holds; `None` when it is not one, an error when it is one whose payload is not.
  pub fn from_value(value: &Value) -> Option<Result<Self, String>> {
    let Value::Variant { tag, payload } = value else { return None };
    if tag != RESPONSE_TAG {
      return None;
    }
    Some(Self::read(payload.as_deref()))
  }

  fn read(payload: Option<&Value>) -> Result<Self, String> {
    let Some(Value::Map(fields)) = payload else { return Err("a response without its fields".to_owned()) };
    let status = match fields.get("status") {
      Some(Value::Int(n)) => u16::try_from(*n).map_err(|_| format!("a response status of {n}"))?,
      other => return Err(format!("a response status of {other:?}")),
    };
    let mut headers = Vec::new();
    if let Some(Value::Seq(given)) = fields.get("headers") {
      for pair in given.iter() {
        match pair {
          Value::Seq(pair) => match (pair.get(0), pair.get(1)) {
            (Some(Value::Str(name)), Some(Value::Str(value))) => headers.push((name.to_string(), value.to_string())),
            _ => return Err("a response header that is not a name and a value".to_owned()),
          },
          _ => return Err("a response header that is not a name and a value".to_owned()),
        }
      }
    }
    let body = match fields.get("body") {
      None | Some(Value::Null | Value::Undefined) => Vec::new(),
      Some(Value::Bytes(bytes)) => bytes.clone(),
      Some(Value::TypedArray(TypedArray::U8(bytes))) => bytes.clone(),
      Some(Value::Str(text)) => text.as_bytes().to_vec(),
      Some(other) => return Err(format!("a response body of {other:?}")),
    };
    let downstream = match fields.get("downstream") {
      Some(Value::Int(at)) => usize::try_from(*at).ok(),
      _ => None,
    };
    Ok(Self { status, headers, body, downstream })
  }
}
