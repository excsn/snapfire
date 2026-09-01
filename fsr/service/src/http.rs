use std::collections::HashMap;
use std::time::Duration;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

use crate::call::Call;
use crate::transport::Transport;

/// How one contract method reaches the wire. Defaults to
/// `POST {base}/{service}/{method}` with the arguments as a JSON body, which
/// is what a plain gateway wants; `route` overrides per method.
#[derive(Debug, Clone)]
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
  /// in the body.
  fn render(&self, args: &ValueMap) -> (String, Vec<String>) {
    let mut path = String::new();
    let mut consumed = Vec::new();
    let mut rest = self.path.as_str();
    while let Some(open) = rest.find('{') {
      let Some(close) = rest[open..].find('}') else { break };
      let name = &rest[open + 1..open + close];
      path.push_str(&rest[..open]);
      if let Some(value) = args.get(name) {
        path.push_str(&scalar_to_path(value));
        consumed.push(name.to_owned());
      }
      rest = &rest[open + close + 1..];
    }
    path.push_str(rest);
    (path, consumed)
  }
}

fn scalar_to_path(value: &Value) -> String {
  match value {
    Value::Str(s) => s.clone(),
    Value::Int(v) => v.to_string(),
    Value::UInt(v) => v.to_string(),
    Value::Bool(b) => b.to_string(),
    Value::F32(v) => v.to_string(),
    Value::F64(v) => v.to_string(),
    other => format!("{other:?}"),
  }
}

pub fn kind_for_status(status: u16) -> FailureKind {
  match status {
    401 | 403 => FailureKind::Unauthorized,
    404 => FailureKind::NotFound,
    409 => FailureKind::Conflict,
    408 | 504 => FailureKind::Timeout,
    502 | 503 => FailureKind::Unavailable,
    400..=499 => FailureKind::Invalid,
    _ => FailureKind::Internal,
  }
}

/// The first transport that leaves the process. Metadata written by the
/// interceptor chain becomes request headers, so identity and the bearer token
/// arrive without application code naming either.
pub struct HttpTransport {
  base: String,
  routes: HashMap<String, Route>,
  client: reqwest::Client,
}

impl HttpTransport {
  pub fn new(base: impl Into<String>) -> Self {
    Self::with_client(base, reqwest::Client::new())
  }

  pub fn with_timeout(base: impl Into<String>, timeout: Duration) -> Self {
    let client = reqwest::Client::builder().timeout(timeout).build().expect("http client builds");
    Self::with_client(base, client)
  }

  pub fn with_client(base: impl Into<String>, client: reqwest::Client) -> Self {
    Self { base: base.into().trim_end_matches('/').to_owned(), routes: HashMap::new(), client }
  }

  pub fn route(mut self, path: impl Into<String>, route: Route) -> Self {
    self.routes.insert(path.into(), route);
    self
  }

  fn plan(&self, call: &Call) -> (String, String, ValueMap) {
    let key = format!("{}.{}", call.service, call.method);
    let mut args = call.args.clone();
    match self.routes.get(&key) {
      Some(route) => {
        let (path, consumed) = route.render(&args);
        for name in consumed {
          args.shift_remove(&name);
        }
        (route.method.clone(), format!("{}{}", self.base, path), args)
      }
      None => (
        "POST".to_owned(),
        format!("{}/{}/{}", self.base, call.service, call.method),
        args,
      ),
    }
  }
}

impl Transport for HttpTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let (method, url, body) = self.plan(&call);
    let service = call.service.clone();
    let name = call.method.clone();
    let fail = move |kind: FailureKind, message: String| {
      ServiceError::new(kind, service.clone(), name.clone(), message)
    };

    let Ok(verb) = reqwest::Method::from_bytes(method.as_bytes()) else {
      return Box::pin(async move { Err(fail(FailureKind::Internal, format!("bad method `{method}`"))) });
    };
    let mut request = self.client.request(verb.clone(), &url);
    for (key, value) in &call.metadata {
      if let Value::Str(value) = value {
        request = request.header(key.as_str(), value.as_str());
      }
    }
    if verb != reqwest::Method::GET && verb != reqwest::Method::DELETE {
      request = request.json(&value_to_json(&Value::Map(body)));
    } else {
      let query: Vec<(String, String)> =
        body.iter().map(|(k, v)| (k.clone(), scalar_to_path(v))).collect();
      request = request.query(&query);
    }

    Box::pin(async move {
      let response = request.send().await.map_err(|e| {
        let kind = if e.is_timeout() { FailureKind::Timeout } else { FailureKind::Unavailable };
        fail(kind, e.to_string())
      })?;
      let status = response.status().as_u16();
      let text = response.text().await.map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      if !(200..300).contains(&status) {
        return Err(fail(kind_for_status(status), format!("{status}: {}", text.trim())));
      }
      if text.trim().is_empty() {
        return Ok(Value::Null);
      }
      let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      json_to_value(&json).map_err(|e| fail(FailureKind::Internal, e.to_string()))
    })
  }
}
