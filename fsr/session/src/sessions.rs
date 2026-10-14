use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_runtime::{unix_now, SessionCell};

use crate::codec::{CookieCodec, HmacCodec};
use crate::csrf::{CsrfScheme, SingleUse};
use crate::store::{SessionRecord, SessionStore, StoreError};
use crate::tokens::TokenCell;
use crate::SessionId;

/// The cookie [`Sessions::state_cookie`] writes. Fixed rather than derived from the session cookie's name, since the page reads it blind.
pub const STATE_COOKIE: &str = "sf_state";

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
/// AUTH.md; `csrf` is the CSRF scheme's own state; `fresh` means no valid
/// cookie arrived.
#[derive(Clone)]
pub struct Opened {
  pub id: SessionId,
  pub cell: SessionCell,
  pub tokens: TokenCell,
  pub csrf: TokenCell,
  pub fresh: bool,
  /// The cookie decoded under something the codec no longer writes, a
  /// retired-in-waiting key, so `persist` sets it again under the current one.
  pub stale: bool,
}

/// The session layer facade: `open` before matching, `persist` when the
/// response starts. Lives at the HTTP adapter edge, since cookies are HTTP.
pub struct Sessions {
  store: Arc<dyn SessionStore>,
  codec: Arc<dyn CookieCodec>,
  /// The CSRF scheme, `SingleUse` unless `with_csrf` chose another.
  csrf: Arc<dyn CsrfScheme>,
  config: SessionConfig,
}

impl Sessions {
  pub fn new(store: Arc<dyn SessionStore>, key: &[u8], config: SessionConfig) -> Self {
    Self::with_codec(store, key, Arc::new(HmacCodec::new(key)), config)
  }

  /// The same layer over a codec of the caller's own, which is what makes
  /// `CookieCodec` a seam rather than a description.
  pub fn with_codec(
    store: Arc<dyn SessionStore>,
    key: &[u8],
    codec: Arc<dyn CookieCodec>,
    config: SessionConfig,
  ) -> Self {
    let _ = key;
    Self { store, codec, csrf: Arc::new(SingleUse::default()), config }
  }

  /// The same layer over a CSRF scheme of the caller's own: one of the three
  /// the crate ships or an implementation of `CsrfScheme`.
  pub fn with_csrf(mut self, scheme: Arc<dyn CsrfScheme>) -> Self {
    self.csrf = scheme;
    self
  }

  /// The cookie's value, unquoted and percent-decoded. RFC 6265 splits pairs
  /// on `;` and a name must match whole rather than by prefix; a cookie
  /// named `sf_session_old` would answer for `sf_session`.
  fn cookie_value(&self, cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|pair| {
      let (name, value) = pair.split_once('=')?;
      if name.trim() != self.config.cookie_name {
        return None;
      }
      let value = value.trim();
      let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
      Some(percent_decode(value))
    })
  }

  pub async fn open(&self, cookie_header: Option<&str>) -> Opened {
    if let Some((id, value)) = cookie_header
      .and_then(|h| self.cookie_value(h))
      .and_then(|v| self.codec.decode(&v).map(|id| (id, v)))
    {
      let stale = !self.codec.current(&value);
      if let Some(record) = self.store.load(&id).await {
        if record.expires > unix_now() {
          return Opened {
            id,
            cell: SessionCell::new(record.data, record.identity).expiring(record.expires),
            tokens: TokenCell::new(record.tokens),
            csrf: TokenCell::new(record.csrf),
            fresh: false,
            stale,
          };
        }
        let _ = self.store.delete(&id).await;
      }
      return Opened { id, cell: self.new_cell(), tokens: TokenCell::default(), csrf: TokenCell::default(), fresh: false, stale };
    }
    Opened { id: SessionId::generate(), cell: self.new_cell(), tokens: TokenCell::default(), csrf: TokenCell::default(), fresh: true, stale: false }
  }

  /// An empty cell ending one `ttl` from now.
  fn new_cell(&self) -> SessionCell {
    SessionCell::default().expiring(unix_now() + self.config.ttl.as_secs())
  }

  /// `Max-Age` counts down to the cell's end.
  fn set_cookie(&self, opened: &Opened) -> String {
    let secure = if self.config.secure { "; Secure" } else { "" };
    format!(
      "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
      self.config.cookie_name,
      self.codec.encode(&opened.id),
      opened.cell.expires().saturating_sub(unix_now()),
      secure
    )
  }

  fn record(opened: &Opened) -> SessionRecord {
    let (data, identity) = opened.cell.snapshot();
    SessionRecord {
      data,
      identity,
      tokens: opened.tokens.snapshot(),
      csrf: opened.csrf.snapshot(),
      expires: opened.cell.expires(),
    }
  }

  /// Saves a dirty or extended cell and returns the `Set-Cookie` value a
  /// fresh, stale or extended session needs: fresh so the browser has the
  /// id, stale so the cookie moves to the current key, extended so its
  /// `Max-Age` reaches the new end. A fresh session that stored nothing sets
  /// no cookie, so crawlers never mint sessions.
  pub async fn persist(&self, opened: &Opened) -> Result<Option<String>, StoreError> {
    let extended = opened.cell.is_extended();
    if !opened.cell.is_dirty() && !opened.tokens.is_dirty() && !opened.csrf.is_dirty() && !extended {
      return Ok(opened.stale.then(|| self.set_cookie(opened)));
    }
    self.store.save(&opened.id, Self::record(opened)).await?;
    Ok((opened.fresh || opened.stale || extended).then(|| self.set_cookie(opened)))
  }

  /// The `Set-Cookie` that tells the page its session moved: `sf_state` with
  /// a fresh generation, readable by script, set beside the session cookie
  /// whenever a written session is saved and again when it is destroyed. A
  /// browser cache keyed on it, the client's router cache for one, drops what
  /// it fetched under the previous generation, whatever path did the writing.
  pub fn state_cookie(&self) -> String {
    let secure = if self.config.secure { "; Secure" } else { "" };
    let generation = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_nanos())
      .unwrap_or(0);
    format!("{STATE_COOKIE}={generation:x}; Path=/; SameSite=Lax; Max-Age={}{}", self.config.ttl.as_secs(), secure)
  }

  /// Logout: deletes the record and returns the expiring cookie.
  /// Saves the session whether or not it changed, so its id survives to the
  /// next request; the cookie to set when the session is fresh. For a host
  /// that bound something to the id, such as a CSRF token, before the
  /// session held anything.
  pub async fn establish(&self, opened: &Opened) -> Result<Option<String>, StoreError> {
    self.store.save(&opened.id, Self::record(opened)).await?;
    Ok((opened.fresh || opened.stale || opened.cell.is_extended()).then(|| self.set_cookie(opened)))
  }

  pub async fn destroy(&self, opened: &Opened) -> Result<String, StoreError> {
    self.store.delete(&opened.id).await?;
    opened.cell.clear();
    opened.tokens.clear();
    opened.csrf.clear();
    self.csrf.rotate(opened);
    let secure = if self.config.secure { "; Secure" } else { "" };
    Ok(format!(
      "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
      self.config.cookie_name, secure
    ))
  }

  /// A token for this request, from the scheme; a single-use scheme mints a
  /// fresh one every time.
  pub fn csrf_token(&self, opened: &Opened) -> String {
    self.csrf.issue(opened)
  }

  pub fn verify_csrf(&self, opened: &Opened, token: &str) -> bool {
    self.csrf.verify(opened, token)
  }

  /// Tells the scheme the session's standing changed, which the edge calls
  /// once a session is identified; `destroy` calls it itself.
  pub fn rotate_csrf(&self, opened: &Opened) {
    self.csrf.rotate(opened);
  }

  pub fn csrf_scheme(&self) -> Arc<dyn CsrfScheme> {
    self.csrf.clone()
  }
}

/// `%xx` escapes decoded; anything that is not a complete escape is kept as
/// written, since a cookie value is not required to be encoded at all.
fn percent_decode(value: &str) -> String {
  let bytes = value.as_bytes();
  let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
      (b'%', Some(hi), Some(lo)) => match (hex(*hi), hex(*lo)) {
        (Some(hi), Some(lo)) => {
          out.push(hi << 4 | lo);
          i += 3;
        }
        _ => {
          out.push(bytes[i]);
          i += 1;
        }
      },
      _ => {
        out.push(bytes[i]);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
  match byte {
    b'0'..=b'9' => Some(byte - b'0'),
    b'a'..=b'f' => Some(byte - b'a' + 10),
    b'A'..=b'F' => Some(byte - b'A' + 10),
    _ => None,
  }
}
