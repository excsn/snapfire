use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_runtime::SessionCell;

use crate::codec::{CookieCodec, HmacCodec};
use crate::store::{SessionRecord, SessionStore};
use crate::tokens::TokenCell;
use crate::SessionId;

pub struct SessionConfig {
  pub cookie_name: String,
  pub ttl: Duration,
  pub secure: bool,
}

impl Default for SessionConfig {
  fn default() -> Self {
    Self { cookie_name: "sf_session".to_owned(), ttl: Duration::from_secs(8 * 3600), secure: false }
  }
}

/// One request's session as the layer sees it. `cell` is what flows into
/// `RequestCtx`; `tokens` never does, which is the custody boundary from
/// AUTH.md; `fresh` means no valid cookie arrived.
pub struct Opened {
  pub id: SessionId,
  pub cell: SessionCell,
  pub tokens: TokenCell,
  pub fresh: bool,
}

/// The session layer facade: `open` before matching, `persist` when the
/// response starts. Lives at the HTTP adapter edge, since cookies are HTTP.
pub struct Sessions {
  store: Arc<dyn SessionStore>,
  codec: HmacCodec,
  config: SessionConfig,
}

impl Sessions {
  pub fn new(store: Arc<dyn SessionStore>, key: &[u8], config: SessionConfig) -> Self {
    Self { store, codec: HmacCodec::new(key), config }
  }

  fn cookie_value<'h>(&self, cookie_header: &'h str) -> Option<&'h str> {
    cookie_header.split(';').find_map(|pair| {
      pair.trim().strip_prefix(self.config.cookie_name.as_str())?.strip_prefix('=')
    })
  }

  pub async fn open(&self, cookie_header: Option<&str>) -> Opened {
    if let Some(id) = cookie_header
      .and_then(|h| self.cookie_value(h))
      .and_then(|v| self.codec.decode(v))
    {
      if let Some(record) = self.store.load(&id).await {
        return Opened {
          id,
          cell: SessionCell::new(record.data, record.identity),
          tokens: TokenCell::new(record.tokens),
          fresh: false,
        };
      }
      return Opened { id, cell: SessionCell::default(), tokens: TokenCell::default(), fresh: false };
    }
    Opened { id: SessionId::generate(), cell: SessionCell::default(), tokens: TokenCell::default(), fresh: true }
  }

  fn set_cookie(&self, id: &SessionId) -> String {
    let secure = if self.config.secure { "; Secure" } else { "" };
    format!(
      "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
      self.config.cookie_name,
      self.codec.encode(id),
      self.config.ttl.as_secs(),
      secure
    )
  }

  /// Saves a dirty cell and returns the `Set-Cookie` value a fresh session
  /// needs. A fresh session that stored nothing sets no cookie, so crawlers
  /// never mint sessions.
  pub async fn persist(&self, opened: &Opened) -> Option<String> {
    if !opened.cell.is_dirty() && !opened.tokens.is_dirty() {
      return None;
    }
    let (data, identity) = opened.cell.snapshot();
    let tokens = opened.tokens.snapshot();
    self.store.save(&opened.id, SessionRecord { data, identity, tokens }).await;
    opened.fresh.then(|| self.set_cookie(&opened.id))
  }

  /// Logout: deletes the record and returns the expiring cookie.
  pub async fn destroy(&self, opened: &Opened) -> String {
    self.store.delete(&opened.id).await;
    let secure = if self.config.secure { "; Secure" } else { "" };
    format!(
      "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
      self.config.cookie_name, secure
    )
  }

  pub fn csrf_token(&self, id: &SessionId) -> String {
    self.codec.sign(format!("csrf:{}", id.0).as_bytes())
  }

  pub fn verify_csrf(&self, id: &SessionId, token: &str) -> bool {
    self.codec.verify(format!("csrf:{}", id.0).as_bytes(), token)
  }
}
