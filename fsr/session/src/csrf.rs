//! CSRF tokens as a scheme the session layer calls and a developer may
//! replace. A scheme issues a token for a request, verifies one a request
//! sent back and is told when the session's standing changed. Its state
//! lives in the session's `csrf` cell, which nothing else reads.

use snapfire_fsr_core::Value;

use crate::codec::HmacCodec;
use crate::{to_hex, Opened};

/// What the layer asks of a CSRF scheme. `issue` and `verify` are the
/// request's two ends; `rotate` is called when the session is identified
/// and when it is destroyed, so a token learned before either is worthless
/// after. `memo` is what the render memo keys a subtree that renders the
/// token by: the token itself when a session's token is stable, `None` when
/// every issue differs, which keeps such a subtree out of the memo.
pub trait CsrfScheme: Send + Sync {
  fn issue(&self, opened: &Opened) -> String;

  fn verify(&self, opened: &Opened, token: &str) -> bool;

  fn rotate(&self, opened: &Opened) {
    let _ = opened;
  }

  fn memo(&self, opened: &Opened) -> Option<String>;
}

/// 32 random bytes as hex, the token a scheme mints when it mints one.
pub fn random_token() -> String {
  let bytes: [u8; 32] = rand::random();
  to_hex(&bytes)
}

/// Equal without an early exit on the first differing byte.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
  let (a, b) = (a.as_bytes(), b.as_bytes());
  let mut diff = a.len() ^ b.len();
  for i in 0..a.len().max(b.len()) {
    diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
  }
  diff == 0
}

/// The token is `hmac(key, "csrf:" + session id)`: nothing is stored, one
/// value is valid for the session id's life and `rotate` changes nothing.
/// Over a `Keyring`, a token minted under a previous key verifies until that
/// key is retired.
pub struct Derived {
  signer: HmacCodec,
}

impl Derived {
  pub fn new(key: &[u8]) -> Self {
    Self { signer: HmacCodec::new(key) }
  }

  /// Over a ring the caller holds, the one the cookie codec signs with or
  /// another.
  pub fn over(ring: std::sync::Arc<crate::Keyring>) -> Self {
    Self { signer: HmacCodec::over(ring) }
  }

  fn input(opened: &Opened) -> Vec<u8> {
    format!("csrf:{}", opened.id.0).into_bytes()
  }
}

impl CsrfScheme for Derived {
  fn issue(&self, opened: &Opened) -> String {
    self.signer.sign(&Self::input(opened))
  }

  fn verify(&self, opened: &Opened, token: &str) -> bool {
    self.signer.verify(&Self::input(opened), token)
  }

  fn memo(&self, opened: &Opened) -> Option<String> {
    Some(self.issue(opened))
  }
}

const TOKEN: &str = "token";
const OUTSTANDING: &str = "outstanding";

/// One random token per session, kept in the record, minted on the first
/// issue and replaced by `rotate`.
#[derive(Default)]
pub struct PerSession;

impl PerSession {
  pub fn new() -> Self {
    Self
  }

  fn current(opened: &Opened) -> Option<String> {
    match opened.csrf.get(TOKEN) {
      Some(Value::Str(token)) => Some(token.to_string()),
      _ => None,
    }
  }
}

impl CsrfScheme for PerSession {
  fn issue(&self, opened: &Opened) -> String {
    match Self::current(opened) {
      Some(token) => token,
      None => {
        let token = random_token();
        opened.csrf.set(TOKEN, Value::str(token.clone()));
        token
      }
    }
  }

  fn verify(&self, opened: &Opened, token: &str) -> bool {
    Self::current(opened).is_some_and(|held| constant_time_eq(&held, token))
  }

  fn rotate(&self, opened: &Opened) {
    opened.csrf.set(TOKEN, Value::str(random_token()));
  }

  fn memo(&self, opened: &Opened) -> Option<String> {
    Some(self.issue(opened))
  }
}

/// A fresh random token per issue, each good for one verification. The
/// record keeps the newest `keep` outstanding tokens, so that many forms may
/// be open at once; an older one is dropped as the newer ones are minted.
pub struct SingleUse {
  keep: usize,
}

impl SingleUse {
  pub fn new(keep: usize) -> Self {
    Self { keep: keep.max(1) }
  }

  fn outstanding(opened: &Opened) -> Vec<String> {
    match opened.csrf.get(OUTSTANDING) {
      Some(Value::Seq(items)) => items
        .iter()
        .filter_map(|item| match item {
          Value::Str(token) => Some(token.to_string()),
          _ => None,
        })
        .collect(),
      _ => Vec::new(),
    }
  }

  fn hold(opened: &Opened, tokens: Vec<String>) {
    opened.csrf.set(OUTSTANDING, Value::Seq(tokens.into_iter().map(Value::str).collect::<Vec<_>>().into()));
  }
}

impl Default for SingleUse {
  fn default() -> Self {
    Self::new(8)
  }
}

impl CsrfScheme for SingleUse {
  fn issue(&self, opened: &Opened) -> String {
    let token = random_token();
    let mut tokens = Self::outstanding(opened);
    tokens.insert(0, token.clone());
    tokens.truncate(self.keep);
    Self::hold(opened, tokens);
    token
  }

  fn verify(&self, opened: &Opened, token: &str) -> bool {
    let tokens = Self::outstanding(opened);
    let Some(at) = tokens.iter().position(|held| constant_time_eq(held, token)) else {
      return false;
    };
    let mut tokens = tokens;
    tokens.remove(at);
    Self::hold(opened, tokens);
    true
  }

  fn rotate(&self, opened: &Opened) {
    if opened.csrf.get(OUTSTANDING).is_some() {
      opened.csrf.remove(OUTSTANDING);
    }
  }

  fn memo(&self, _opened: &Opened) -> Option<String> {
    None
  }
}
