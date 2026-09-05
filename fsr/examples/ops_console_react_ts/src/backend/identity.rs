//! The identity service: the accounts and every session live here, reached
//! only over HTTP and described only by the document it publishes. The host
//! keeps neither.

use std::collections::HashMap;
use std::sync::Arc;

use actix_web::dev::Server;
use actix_web::web::{Data, Json, Path};
use actix_web::{web, App, HttpResponse, HttpServer};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Included from `app/clients/` rather than copied, so the document the build
/// imports and the document this server publishes cannot disagree.
pub const DOCUMENT: &str = include_str!("../../app/clients/identity.openapi.json");

/// The same accounts file the `file` provider would read; here the service
/// owns it and the host never opens it.
const ACCOUNTS: &str = include_str!("../../config/auth.toml");

#[derive(Deserialize)]
struct Accounts {
  users: Vec<Account>,
}

#[derive(Clone, Deserialize)]
struct Account {
  name: String,
  password: String,
  #[serde(default)]
  claims: HashMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct Credentials {
  user: String,
  password: String,
}

#[derive(Serialize)]
struct Signed {
  subject: String,
  claims: HashMap<String, String>,
  access_token: String,
}

#[derive(Serialize, Deserialize)]
struct Stored {
  record: String,
}

pub struct Identity {
  accounts: Vec<Account>,
  sessions: RwLock<HashMap<String, String>>,
}

impl Identity {
  pub fn seed() -> Arc<Self> {
    let accounts: Accounts = toml::from_str(ACCOUNTS).expect("config/auth.toml parses");
    Arc::new(Self { accounts: accounts.users, sessions: RwLock::new(HashMap::new()) })
  }

  pub fn session_count(&self) -> usize {
    self.sessions.read().len()
  }

  pub fn session(&self, id: &str) -> Option<String> {
    self.sessions.read().get(id).cloned()
  }

  /// Every record, for a test that does not know the cookie's session id.
  pub fn sessions_dump(&self) -> String {
    self.sessions.read().values().cloned().collect::<Vec<_>>().join("\n")
  }
}

async fn authenticate(identity: Data<Arc<Identity>>, body: Json<Credentials>) -> HttpResponse {
  match identity.accounts.iter().find(|a| a.name == body.user && a.password == body.password) {
    Some(account) => {
      let claims = account.claims.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))).collect();
      HttpResponse::Ok().json(Signed { subject: account.name.clone(), claims, access_token: format!("svc-token-{}", account.name) })
    }
    None => HttpResponse::Unauthorized().body("unknown user or wrong password"),
  }
}

async fn get_session(identity: Data<Arc<Identity>>, id: Path<String>) -> HttpResponse {
  match identity.sessions.read().get(id.as_str()) {
    Some(record) => HttpResponse::Ok().json(Stored { record: record.clone() }),
    None => HttpResponse::NotFound().body(format!("no session {}", *id)),
  }
}

async fn put_session(identity: Data<Arc<Identity>>, id: Path<String>, body: Json<Stored>) -> HttpResponse {
  identity.sessions.write().insert(id.into_inner(), body.into_inner().record);
  HttpResponse::NoContent().finish()
}

async fn delete_session(identity: Data<Arc<Identity>>, id: Path<String>) -> HttpResponse {
  identity.sessions.write().remove(id.as_str());
  HttpResponse::NoContent().finish()
}

async fn document() -> HttpResponse {
  HttpResponse::Ok().content_type("application/json; charset=utf-8").body(DOCUMENT)
}

/// Binds the service and hands back the port it took, so a test can ask for
/// port 0.
pub fn bind(identity: Arc<Identity>, addr: (&str, u16)) -> std::io::Result<(u16, Server)> {
  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(identity.clone()))
      .route("/openapi.json", web::get().to(document))
      .route("/authenticate", web::post().to(authenticate))
      .route("/sessions/{id}", web::get().to(get_session))
      .route("/sessions/{id}", web::put().to(put_session))
      .route("/sessions/{id}", web::delete().to(delete_session))
  })
  .bind(addr)?;
  let port = server.addrs().first().map(|a| a.port()).unwrap_or(addr.1);
  Ok((port, server.run()))
}

pub async fn serve(identity: Arc<Identity>, addr: (&'static str, u16)) -> std::io::Result<()> {
  bind(identity, addr)?.1.await
}
