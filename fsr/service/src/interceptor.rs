use std::sync::Arc;

use futures_util::future::BoxFuture;
use tracing::Instrument;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::ServiceError;

use crate::call::Call;
use crate::transport::Transport;

/// One step of the outbound path: an ordered list of functions, tower-style,
/// not a workflow engine. See SERVICES.md section 3b.
pub trait Interceptor: Send + Sync {
  fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>>;
}

pub(crate) struct Chain {
  pub(crate) interceptors: Vec<Arc<dyn Interceptor>>,
  pub(crate) transport: Arc<dyn Transport>,
}

/// The rest of the chain. Calling `run` continues; not calling it short
/// circuits, which is how a cache or a circuit breaker would sit here.
#[derive(Clone)]
pub struct Next {
  chain: Arc<Chain>,
  index: usize,
}

impl Next {
  pub(crate) fn start(chain: Arc<Chain>) -> Self {
    Self { chain, index: 0 }
  }

  /// The chain from `index` on, which is how the data cache re-enters it.
  pub(crate) fn at(chain: Arc<Chain>, index: usize) -> Self {
    Self { chain, index }
  }

  pub fn run(self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    match self.chain.interceptors.get(self.index) {
      Some(interceptor) => {
        let next = Next { chain: self.chain.clone(), index: self.index + 1 };
        interceptor.call(call, next)
      }
      None => self.chain.transport.call(call),
    }
  }
}

/// Propagates who the request is onto every outbound call.
pub struct IdentityInterceptor {
  key: String,
}

impl IdentityInterceptor {
  pub fn new() -> Self {
    Self { key: "x-sf-subject".to_owned() }
  }

  pub fn key(mut self, key: impl Into<String>) -> Self {
    self.key = key.into();
    self
  }
}

impl Default for IdentityInterceptor {
  fn default() -> Self {
    Self::new()
  }
}

impl Interceptor for IdentityInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if let Some(identity) = call.identity.clone() {
      call.set_metadata(self.key.clone(), identity.subject);
    }
    next.run(call)
  }
}

/// Renews the credentials custody holds: the identity provider's `refresh`, given every credential
/// and answering the ones that changed; `None` when it renews nothing.
pub trait Refresher: Send + Sync {
  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'static, Option<ValueMap>>;
}

/// How long before `expires_at` a credential counts as expired, so it is not refused in flight.
pub const EXPIRY_MARGIN_SECS: u64 = 30;

/// Reads one credential out of custody and attaches it. This is the step that
/// makes "application code never sees a token" structural. With a refresher it
/// renews the credentials first when custody's `expires_at` has passed, and
/// once after a call is refused as unauthorized, running the call again.
pub struct CredentialInterceptor {
  credential: String,
  header: String,
  scheme: String,
  /// The services the credential goes to; every service when empty.
  services: Vec<String>,
  refresher: Option<Arc<dyn Refresher>>,
}

impl CredentialInterceptor {
  pub fn bearer(credential: impl Into<String>) -> Self {
    Self {
      credential: credential.into(),
      header: "authorization".to_owned(),
      scheme: "Bearer ".to_owned(),
      services: Vec::new(),
      refresher: None,
    }
  }

  pub fn refreshing(mut self, refresher: Arc<dyn Refresher>) -> Self {
    self.refresher = Some(refresher);
    self
  }

  pub fn header(mut self, header: impl Into<String>) -> Self {
    self.header = header.into();
    self
  }

  pub fn scheme(mut self, scheme: impl Into<String>) -> Self {
    self.scheme = scheme.into();
    self
  }

  /// Attaches the credential only on calls to the named services.
  pub fn only(mut self, services: impl IntoIterator<Item = impl Into<String>>) -> Self {
    self.services = services.into_iter().map(Into::into).collect();
    self
  }
}

impl CredentialInterceptor {
  fn attach(&self, call: &mut Call) {
    if let Some(Value::Str(token)) = call.credentials.get(&self.credential) {
      call.set_metadata(self.header.clone(), format!("{}{token}", self.scheme));
    }
  }
}

/// Asks `refresher` to renew `credentials` and writes back what it answers; whether it renewed anything.
async fn renew(refresher: &Arc<dyn Refresher>, credentials: &Arc<dyn crate::call::Credentials>) -> bool {
  match refresher.refresh(credentials.snapshot()).await {
    Some(renewed) if !renewed.is_empty() => {
      for (key, value) in renewed {
        credentials.set(&key, value);
      }
      true
    }
    _ => false,
  }
}

fn expired(credentials: &dyn crate::call::Credentials) -> bool {
  let at = match credentials.get("expires_at") {
    Some(Value::Int(at)) => u64::try_from(at).unwrap_or(0),
    Some(Value::UInt(at)) => u64::try_from(at).unwrap_or(u64::MAX),
    Some(Value::F64(at)) if at.is_finite() && at >= 0.0 => at as u64,
    _ => return false,
  };
  at <= snapfire_fsr_runtime::unix_now() + EXPIRY_MARGIN_SECS
}

impl Interceptor for CredentialInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let named = self.services.is_empty() || self.services.iter().any(|s| *s == call.service);
    if !named {
      return next.run(call);
    }
    let Some(refresher) = self.refresher.clone().filter(|_| call.credentials.get(&self.credential).is_some()) else {
      self.attach(&mut call);
      return next.run(call);
    };
    let this = CredentialInterceptor { credential: self.credential.clone(), header: self.header.clone(), scheme: self.scheme.clone(), services: Vec::new(), refresher: None };
    Box::pin(async move {
      let mut renewed = false;
      if expired(call.credentials.as_ref()) {
        renewed = renew(&refresher, &call.credentials).await;
      }
      this.attach(&mut call);
      let again = call.again();
      let answer = next.clone().run(call).await;
      match answer {
        Err(error) if error.kind == snapfire_fsr_runtime::FailureKind::Unauthorized && !renewed && renew(&refresher, &again.credentials).await => {
          let mut call = again;
          this.attach(&mut call);
          next.run(call).await
        }
        answer => answer,
      }
    })
  }
}

/// A request id that survives the whole fanout, minted once per request and
/// reused by every call that carries it in.
pub struct TraceInterceptor {
  key: String,
}

impl TraceInterceptor {
  pub fn new() -> Self {
    Self { key: "x-sf-request-id".to_owned() }
  }

  pub fn key(mut self, key: impl Into<String>) -> Self {
    self.key = key.into();
    self
  }
}

impl Default for TraceInterceptor {
  fn default() -> Self {
    Self::new()
  }
}

impl Interceptor for TraceInterceptor {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if call.metadata_str(&self.key).is_none() {
      call.set_metadata(self.key.clone(), format!("{:016x}", call.request));
    }
    tracing::debug!(
      target: "fsr::service",
      service = call.service.as_str(),
      method = call.method.as_str(),
      request_id = call.metadata_str(&self.key).unwrap_or_default(),
      "call"
    );
    // The call span. A collector reads the status kind rather than a raw
    // status, since the failure vocabulary is what a caller acts on.
    let span = tracing::info_span!(
      target: "fsr::trace",
      "call",
      service = call.service.as_str(),
      method = call.method.as_str(),
      cache = "none",
      fibre.outcome = tracing::field::Empty,
    );
    let answered = next.run(call);
    Box::pin(
      async move {
        let answered = answered.await;
        tracing::Span::current().record(
          "fibre.outcome",
          match &answered {
            Ok(_) => "ok",
            Err(e) => e.kind.as_str(),
          },
        );
        answered
      }
      .instrument(span),
    )
  }
}
