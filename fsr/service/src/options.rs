//! How a transport connects to its backend, one set of options for every transport. A transport refuses an option its backend cannot honour rather than ignoring it.

use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportOptions {
  pub(crate) roots: Vec<Vec<u8>>,
  pub(crate) identity: Option<(Vec<u8>, Vec<u8>)>,
  pub(crate) proxy: Option<String>,
  pub(crate) max_connections: Option<usize>,
  pub(crate) keep_alive: Option<(Duration, Duration)>,
}

impl TransportOptions {
  pub fn new() -> Self {
    Self::default()
  }

  /// Trusts `der`, a DER-encoded certificate, as a root beside the bundled ones: a backend behind a private CA.
  pub fn root_certificate(mut self, der: Vec<u8>) -> Self {
    self.roots.push(der);
    self
  }

  /// Presents `cert_chain_pem` and its `key_pem` to the backend: mutual TLS.
  pub fn identity(mut self, cert_chain_pem: Vec<u8>, key_pem: Vec<u8>) -> Self {
    self.identity = Some((cert_chain_pem, key_pem));
    self
  }

  /// Sends every call through the HTTP proxy at `url`, such as `http://proxy.internal:3128`, in place of the proxies from the environment.
  pub fn proxy(mut self, url: impl Into<String>) -> Self {
    self.proxy = Some(url.into());
    self
  }

  /// The most connections open to the backend at once.
  pub fn max_connections(mut self, n: usize) -> Self {
    self.max_connections = Some(n);
    self
  }

  /// An HTTP/2 PING after `interval` with nothing read while a call is in flight. Without a PONG within `timeout` the connection closes and its calls fail.
  pub fn keep_alive(mut self, interval: Duration, timeout: Duration) -> Self {
    self.keep_alive = Some((interval, timeout));
    self
  }

  /// The options set here that `supported` leaves out, as the first refusal.
  pub(crate) fn refuse(&self, transport: &str, supported: &[&str]) -> Result<(), TransportError> {
    let set = [
      ("root_certificate", !self.roots.is_empty()),
      ("identity", self.identity.is_some()),
      ("proxy", self.proxy.is_some()),
      ("max_connections", self.max_connections.is_some()),
      ("keep_alive", self.keep_alive.is_some()),
    ];
    match set.into_iter().find(|(name, on)| *on && !supported.contains(name)) {
      Some((option, _)) => Err(TransportError::Unsupported { transport: transport.to_owned(), option: option.to_owned() }),
      None => Ok(()),
    }
  }
}

/// Why a transport could not be built from its options.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
  #[error("root certificate: {0}")]
  RootCertificate(String),
  #[error("client identity: {0}")]
  Identity(String),
  #[error("proxy `{url}`: {reason}")]
  Proxy { url: String, reason: String },
  #[error("base URL `{url}`: {reason}")]
  Url { url: String, reason: String },
  #[error("the {transport} transport does not support `{option}`")]
  Unsupported { transport: String, option: String },
}

/// A fibre_http client from `options`, for the transports that run on it.
#[cfg(any(feature = "http", feature = "graphql"))]
pub(crate) fn fibre_client(options: &TransportOptions, transport: &str, max_connections: usize, timeout: Option<Duration>) -> Result<fibre_http::AsyncClient, TransportError> {
  options.refuse(transport, &["root_certificate", "identity", "proxy", "max_connections", "keep_alive"])?;
  let builder = || -> Result<fibre_http::ClientBuilder, TransportError> {
    let pool = fibre_http::Pool::builder().max_per_host(options.max_connections.unwrap_or(max_connections)).build();
    let mut builder = fibre_http::AsyncClient::builder().pool(&pool);
    for der in &options.roots {
      builder = builder.add_root_certificate(der.clone());
    }
    if let Some(url) = &options.proxy {
      let proxy = fibre_http::Proxy::parse(url).map_err(|e| TransportError::Proxy { url: url.clone(), reason: e.to_string() })?;
      builder = builder.proxy(proxy);
    }
    if let Some((interval, pong)) = options.keep_alive {
      builder = builder.http2_keep_alive_interval(interval).http2_keep_alive_timeout(pong);
    }
    if let Some(timeout) = timeout {
      builder = builder.timeout(timeout);
    }
    Ok(builder)
  };
  let client = builder()?.build_async().map_err(|e| TransportError::RootCertificate(e.to_string()))?;
  match &options.identity {
    None => Ok(client),
    Some((chain, key)) => builder()?.identity(chain.clone(), key.clone()).build_async().map_err(|e| TransportError::Identity(e.to_string())),
  }
}
