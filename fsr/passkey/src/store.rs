use std::path::PathBuf;

use futures_util::future::{ready, BoxFuture};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use crate::webauthn::PublicKey;

/// One enrolled passkey: whose it is, the claims it signs in with and its public key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Credential {
  /// The credential id, base64url.
  pub id: String,
  pub subject: String,
  /// The identity's claims when it enrolled, which a sign-in with the passkey carries.
  #[serde(default)]
  pub claims: Map<String, Json>,
  pub key: PublicKey,
  /// The authenticator's signature counter at the last use; a counter that does not move past it
  /// is a cloned authenticator.
  pub sign_count: u32,
}

impl Credential {
  pub fn new(id: String, subject: String, claims: Map<String, Json>, key: PublicKey, sign_count: u32) -> Self {
    Self { id, subject, claims, key, sign_count }
  }
}

/// Where passkeys live. A deployment of several hosts gives every one the same store.
pub trait PasskeyStore: Send + Sync {
  fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>>;
  /// Every credential `subject` enrolled.
  fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>>;
  /// Adds a credential or replaces the one with its id.
  fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>>;
}

/// Passkeys in this process, gone when it stops.
#[derive(Default)]
pub struct MemoryPasskeys {
  credentials: Mutex<Vec<Credential>>,
}

impl MemoryPasskeys {
  pub fn new() -> Self {
    Self::default()
  }
}

fn upsert(credentials: &mut Vec<Credential>, credential: Credential) {
  match credentials.iter_mut().find(|held| held.id == credential.id) {
    Some(held) => *held = credential,
    None => credentials.push(credential),
  }
}

impl PasskeyStore for MemoryPasskeys {
  fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>> {
    Box::pin(ready(Ok(self.credentials.lock().iter().find(|c| c.id == id).cloned())))
  }

  fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>> {
    Box::pin(ready(Ok(self.credentials.lock().iter().filter(|c| c.subject == subject).cloned().collect())))
  }

  fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>> {
    upsert(&mut self.credentials.lock(), credential);
    Box::pin(ready(Ok(())))
  }
}

/// Passkeys in a JSON file, `{ "credentials": [...] }`, read on every call and written whole
/// through a file beside it renamed into place. For one host: two hosts writing one file lose
/// each other's writes.
pub struct FilePasskeys {
  path: PathBuf,
  writing: Mutex<()>,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
  #[serde(default)]
  credentials: Vec<Credential>,
}

impl FilePasskeys {
  pub fn new(path: impl Into<PathBuf>) -> Self {
    Self { path: path.into(), writing: Mutex::new(()) }
  }

  fn read(&self) -> Result<File, String> {
    match std::fs::read_to_string(&self.path) {
      Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", self.path.display())),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(File::default()),
      Err(e) => Err(format!("{}: {e}", self.path.display())),
    }
  }
}

impl PasskeyStore for FilePasskeys {
  fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>> {
    Box::pin(ready(self.read().map(|file| file.credentials.into_iter().find(|c| c.id == id))))
  }

  fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>> {
    Box::pin(ready(self.read().map(|file| file.credentials.into_iter().filter(|c| c.subject == subject).collect())))
  }

  fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>> {
    let saved = (|| {
      let _held = self.writing.lock();
      let mut file = self.read()?;
      upsert(&mut file.credentials, credential);
      let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
      let fresh = self.path.with_extension("json.writing");
      std::fs::write(&fresh, text).map_err(|e| format!("{}: {e}", fresh.display()))?;
      std::fs::rename(&fresh, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    })();
    Box::pin(ready(saved))
  }
}
