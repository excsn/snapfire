use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_core::{TypedArray, Value, ValueMap};
use snapfire_fsr_runtime::{unix_now, SessionCell};

use crate::codec::{CookieCodec, HmacCodec};
use crate::csrf::{CsrfScheme, SingleUse};
use crate::sealed::{SealedCookie, COOKIE_LIMIT};
use crate::store::{SessionRecord, SessionStore, StoreError};
use crate::tokens::TokenCell;
use crate::SessionId;

/// The cookie [`Sessions::state_cookie`] writes. Fixed rather than derived from the session cookie's name, since the page reads it blind.
pub const STATE_COOKIE: &str = "sf_state";

#[non_exhaustive]
pub struct SessionConfig {
  pub cookie_name: String,
  pub ttl: Duration,
  pub secure: bool,
  /// The most a session's data, tokens and CSRF state may hold, about, in bytes; a save over it is refused.
  pub max_bytes: usize,
}

impl SessionConfig {
  pub fn cookie_name(mut self, cookie_name: String) -> Self {
    self.cookie_name = cookie_name;
    self
  }

  pub fn ttl(mut self, ttl: Duration) -> Self {
    self.ttl = ttl;
    self
  }

  pub fn secure(mut self, secure: bool) -> Self {
    self.secure = secure;
    self
  }

  pub fn max_bytes(mut self, max_bytes: usize) -> Self {
    self.max_bytes = max_bytes;
    self
  }
}

impl Default for SessionConfig {
  fn default() -> Self {
    Self { cookie_name: "sf_session".to_owned(), ttl: Duration::from_secs(8 * 3600), secure: false, max_bytes: DEFAULT_MAX_BYTES }
  }
}

/// `session.max_bytes` when unset. The record a signed-in visitor with outstanding form tokens holds is under a kilobyte.
pub const DEFAULT_MAX_BYTES: usize = 16 * 1024;

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

/// Where a session's record lives between requests.
enum Keeping {
  /// In a store, the cookie carrying the id the codec signs.
  Stored { store: Arc<dyn SessionStore>, codec: Arc<dyn CookieCodec> },
  /// In the cookie itself, sealed. Nothing on the server forgets it: a copy of the cookie taken
  /// before `destroy` opens until its record's end.
  Sealed(Arc<SealedCookie>),
}

/// The session layer facade: `open` before matching, `persist` when the
/// response starts. Lives at the HTTP adapter edge, since cookies are HTTP.
pub struct Sessions {
  keeping: Keeping,
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
    Self { keeping: Keeping::Stored { store, codec }, csrf: Arc::new(SingleUse::default()), config }
  }

  /// The layer with no store: each session's record rides in its cookie, sealed by `sealer`, so
  /// any replica behind a load balancer opens it. A save that would make the cookie larger than
  /// [`COOKIE_LIMIT`] is refused.
  pub fn sealed(sealer: Arc<SealedCookie>, config: SessionConfig) -> Self {
    Self { keeping: Keeping::Sealed(sealer), csrf: Arc::new(SingleUse::default()), config }
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
    let (store, codec) = match &self.keeping {
      Keeping::Stored { store, codec } => (store, codec),
      Keeping::Sealed(sealer) => {
        return match cookie_header.and_then(|h| self.cookie_value(h)).and_then(|value| sealer.open(&value)) {
          Some((id, record, position)) if record.expires > unix_now() => Opened {
            id,
            cell: SessionCell::new(record.data, record.identity).expiring(record.expires),
            tokens: TokenCell::new(record.tokens),
            csrf: TokenCell::new(record.csrf),
            fresh: false,
            stale: position != 0,
          },
          _ => self.fresh(),
        };
      }
    };
    if let Some((id, value)) = cookie_header
      .and_then(|h| self.cookie_value(h))
      .and_then(|v| codec.decode(&v).map(|id| (id, v)))
    {
      let stale = !codec.current(&value);
      if let Some(record) = store.load(&id).await {
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
        let _ = store.delete(&id).await;
      }
      return Opened { id, cell: self.new_cell(), tokens: TokenCell::default(), csrf: TokenCell::default(), fresh: false, stale };
    }
    self.fresh()
  }

  fn fresh(&self) -> Opened {
    Opened { id: SessionId::generate(), cell: self.new_cell(), tokens: TokenCell::default(), csrf: TokenCell::default(), fresh: true, stale: false }
  }

  /// An empty cell ending one `ttl` from now.
  fn new_cell(&self) -> SessionCell {
    SessionCell::default().expiring(unix_now() + self.config.ttl.as_secs())
  }

  /// About how many bytes `opened` would store when it is over `max_bytes`, else `None`.
  pub fn too_large(&self, opened: &Opened) -> Option<usize> {
    let (data, _) = opened.cell.snapshot();
    let size = map_bytes(&data) + map_bytes(&opened.tokens.snapshot()) + map_bytes(&opened.csrf.snapshot());
    (size > self.config.max_bytes).then_some(size)
  }

  fn refuse_oversized(&self, opened: &Opened) -> Result<(), StoreError> {
    match self.too_large(opened) {
      Some(size) => Err(StoreError(format!("the session would hold about {size} bytes, over session.max_bytes ({})", self.config.max_bytes))),
      None => Ok(()),
    }
  }

  /// `Max-Age` counts down to the cell's end. A sealed cookie larger than a browser keeps is
  /// refused.
  fn set_cookie(&self, opened: &Opened) -> Result<String, StoreError> {
    let value = match &self.keeping {
      Keeping::Stored { codec, .. } => codec.encode(&opened.id),
      Keeping::Sealed(sealer) => {
        let value = sealer.seal(&opened.id, &Self::record(opened));
        let size = self.config.cookie_name.len() + 1 + value.len();
        if size > COOKIE_LIMIT {
          return Err(StoreError(format!("the session cookie would be {size} bytes, over the {COOKIE_LIMIT} a browser keeps; keep the session in a store")));
        }
        value
      }
    };
    let secure = if self.config.secure { "; Secure" } else { "" };
    Ok(format!(
      "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
      self.config.cookie_name,
      value,
      opened.cell.expires().saturating_sub(unix_now()),
      secure
    ))
  }

  async fn save(&self, opened: &Opened) -> Result<(), StoreError> {
    match &self.keeping {
      Keeping::Stored { store, .. } => store.save(&opened.id, Self::record(opened)).await,
      Keeping::Sealed(_) => Ok(()),
    }
  }

  async fn delete(&self, id: &SessionId) -> Result<(), StoreError> {
    match &self.keeping {
      Keeping::Stored { store, .. } => store.delete(id).await,
      Keeping::Sealed(_) => Ok(()),
    }
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
      return opened.stale.then(|| self.set_cookie(opened)).transpose();
    }
    self.refuse_oversized(opened)?;
    self.save(opened).await?;
    let sealed = matches!(self.keeping, Keeping::Sealed(_));
    (sealed || opened.fresh || opened.stale || extended).then(|| self.set_cookie(opened)).transpose()
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
  /// Saves a fresh session whether or not it changed, so its id survives to the
  /// next request and returns its cookie; an established one is saved as
  /// `persist` saves it. For a host that bound something to the id, such as
  /// a CSRF token, before the session held anything.
  pub async fn establish(&self, opened: &Opened) -> Result<Option<String>, StoreError> {
    if !opened.fresh {
      return self.persist(opened).await;
    }
    self.refuse_oversized(opened)?;
    self.save(opened).await?;
    self.set_cookie(opened).map(Some)
  }

  pub async fn destroy(&self, opened: &Opened) -> Result<String, StoreError> {
    self.delete(&opened.id).await?;
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

  /// The same session under a new id, the old record deleted. Called on sign-in, so an id someone else fixed in the visitor's browser before login never becomes the signed-in one.
  pub async fn renew(&self, opened: &Opened) -> Result<Opened, StoreError> {
    self.delete(&opened.id).await?;
    Ok(Opened { id: SessionId::generate(), cell: opened.cell.clone(), tokens: opened.tokens.clone(), csrf: opened.csrf.clone(), fresh: true, stale: false })
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

fn map_bytes(map: &ValueMap) -> usize {
  map.iter().map(|(key, value)| key.len() + value_bytes(value)).sum()
}

/// About how many bytes `value` takes: text and binary by length, anything scalar a word.
fn value_bytes(value: &Value) -> usize {
  match value {
    Value::Str(text) => text.len(),
    Value::Bytes(bytes) => bytes.len(),
    Value::TypedArray(array) => match array {
      TypedArray::I8(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::U8(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::I16(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::U16(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::I32(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::U32(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::I64(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::U64(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::F32(v) => std::mem::size_of_val(v.as_slice()),
      TypedArray::F64(v) => std::mem::size_of_val(v.as_slice()),
    },
    Value::Seq(items) => items.iter().map(value_bytes).sum::<usize>() + 8,
    Value::Map(map) => map_bytes(map) + 8,
    Value::Variant { tag, payload } => tag.len() + payload.as_deref().map_or(0, value_bytes),
    Value::Ref { id, .. } => id.len(),
    _ => 8,
  }
}
