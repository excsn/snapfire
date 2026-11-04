//! The HTTP transport, behind the `http` feature since it is the one part of the HTTP contract that needs an HTTP client.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

use super::{Route, kind_for_status, replace_lone_surrogates, scalar_to_path};
use crate::call::Call;
use crate::transport::Transport;

/// The first transport that leaves the process. Metadata written by the
/// interceptor chain becomes request headers, so identity and the bearer token
/// arrive without application code naming either.
pub struct HttpTransport {
  base: String,
  routes: HashMap<String, Route>,
  client: reqwest::Client,
  /// Sent on every call before the call's own metadata: an application
  /// credential, which no interceptor or trace sees since it is not metadata.
  headers: Vec<(String, String)>,
  query: Vec<(String, String)>,
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
    Self { base: base.into().trim_end_matches('/').to_owned(), routes: HashMap::new(), client, headers: Vec::new(), query: Vec::new() }
  }

  /// A header every call carries.
  pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.headers.push((name.into(), value.into()));
    self
  }

  /// A query parameter every call carries, beside any its arguments become.
  pub fn query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.query.push((name.into(), value.into()));
    self
  }

  pub fn route(mut self, path: impl Into<String>, route: Route) -> Self {
    self.routes.insert(path.into(), route);
    self
  }

  fn plan(&self, call: &Call) -> Result<(String, String, ValueMap), String> {
    let key = format!("{}.{}", call.service, call.method);
    let mut args = call.args.clone();
    match self.routes.get(&key) {
      Some(route) => {
        let (path, consumed) = route.render(&args)?;
        for name in consumed {
          args.shift_remove(&name);
        }
        Ok((route.method.clone(), format!("{}{}", self.base, path), args))
      }
      None => Ok((
        "POST".to_owned(),
        format!("{}/{}/{}", self.base, call.service, call.method),
        args,
      )),
    }
  }
}

impl Transport for HttpTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let planned = self.plan(&call);
    let service = call.service.clone();
    let name = call.method.clone();
    let fail = move |kind: FailureKind, message: String| {
      ServiceError::new(kind, service.clone(), name.clone(), message)
    };

    let (method, url, body) = match planned {
      Ok(planned) => planned,
      Err(why) => return Box::pin(async move { Err(fail(FailureKind::Invalid, why)) }),
    };

    let Ok(verb) = reqwest::Method::from_bytes(method.as_bytes()) else {
      return Box::pin(async move { Err(fail(FailureKind::Internal, format!("bad method `{method}`"))) });
    };
    let mut request = self.client.request(verb.clone(), &url);
    for (key, value) in &self.headers {
      request = request.header(key.as_str(), value.as_str());
    }
    for (key, value) in &call.metadata {
      let Value::Str(value) = value else {
        let why = format!("metadata `{key}` is not a string, so it cannot be a header");
        return Box::pin(async move { Err(fail(FailureKind::Internal, why)) });
      };
      request = request.header(key.as_str(), value.as_str());
    }
    if verb != reqwest::Method::GET && verb != reqwest::Method::DELETE {
      let text = match snapfire_fsr_core::json::stringify(&Value::Map(body), "", snapfire_fsr_core::json::Integers::Digits) {
        Ok(text) => text,
        Err(e) => return Box::pin(async move { Err(fail(FailureKind::Internal, e.0)) }),
      };
      request = request.header(reqwest::header::CONTENT_TYPE, "application/json").body(text);
    } else {
      let query: Vec<(String, String)> =
        body.iter().map(|(k, v)| (k.to_string(), scalar_to_path(v))).collect();
      request = request.query(&query);
    }
    if !self.query.is_empty() {
      request = request.query(&self.query);
    }

    Box::pin(async move {
      let response = request.send().await.map_err(|e| {
        tracing::warn!(target: "fsr::service", error = %e, "backend request failed");
        match e.is_timeout() {
          true => fail(FailureKind::Timeout, "the backend timed out".to_owned()),
          false => fail(FailureKind::Unavailable, "the backend did not answer".to_owned()),
        }
      })?;
      let status = response.status().as_u16();
      let text = response.text().await.map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      if !(200..300).contains(&status) {
        let kind = kind_for_status(status);
        let says_why = kind.is_client_error() && !matches!(kind, FailureKind::Unauthorized | FailureKind::Forbidden | FailureKind::NotFound | FailureKind::RequestTimeout);
        return Err(match says_why {
          true => fail(kind, format!("{status}: {}", text.trim())),
          false => {
            tracing::warn!(target: "fsr::service", status, body = %text.trim(), "backend answered with a failure");
            fail(kind, status.to_string())
          }
        });
      }
      if text.trim().is_empty() {
        return Ok(Value::Null);
      }
      let json: serde_json::Value =
        serde_json::from_str(&replace_lone_surrogates(&text)).map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      Ok(snapfire_fsr_core::json::to_value(&json))
    })
  }
}
