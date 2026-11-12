//! Which of the host's Cargo features an application calls for, read from its configuration and its files. The build checks them against the binary that will serve the application, and the host checks them against itself at boot.

use crate::config::{Config, TimeZones};
use crate::HostError;

/// The features of the host's `stock` feature, which `fsr serve` and `fsr-server` are both built with.
pub const STOCK: &[&str] = &["auth", "oidc", "passkey", "tera", "tls", "grpc", "graphql", "http", "share_cards", "tz_compiled", "tz_system"];

/// One feature an application calls for and what in it calls for the feature.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Need {
  pub feature: &'static str,
  /// What asks for it, as the configuration or the tree spells it: `[server.tls]`, `clients/shop.proto`.
  pub because: String,
  /// What a crate can do in code instead of turning the feature on.
  pub instead: Option<Instead>,
}

/// What answers a need in code, in place of the feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Instead {
  /// `HostBuilder::services`: the host builds no client transport.
  Services,
  /// `HostBuilder::evaluator`: an evaluator of the crate's own renders the template.
  Evaluator,
}

impl Instead {
  /// How the configuration's need is answered in code, for a message.
  pub fn describe(self) -> &'static str {
    match self {
      Instead::Services => "a host handed its services with `HostBuilder::services` builds no transport",
      Instead::Evaluator => "a template a registered `HostBuilder::evaluator` answers needs no `tera`",
    }
  }
}

impl Need {
  fn new(feature: &'static str, because: impl Into<String>) -> Self {
    Self { feature, because: because.into(), instead: None }
  }

  fn or(feature: &'static str, because: impl Into<String>, instead: Instead) -> Self {
    Self { feature, because: because.into(), instead: Some(instead) }
  }
}

impl std::fmt::Display for Need {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "`{}` for {}", self.feature, self.because)
  }
}

/// Whether this build of the host holds `feature`. A name the host does not define is never held.
pub fn built(feature: &str) -> bool {
  match feature {
    "auth" => cfg!(feature = "auth"),
    "oidc" => cfg!(feature = "oidc"),
    "passkey" => cfg!(feature = "passkey"),
    "tera" => cfg!(feature = "tera"),
    "tls" => cfg!(feature = "tls"),
    "grpc" => cfg!(feature = "grpc"),
    "graphql" => cfg!(feature = "graphql"),
    "http" => cfg!(feature = "http"),
    "share_cards" => cfg!(feature = "share_cards"),
    "tz_compiled" => cfg!(feature = "tz_compiled"),
    "tz_system" => cfg!(feature = "tz_system"),
    "ws" => cfg!(feature = "ws"),
    "actix" => cfg!(feature = "actix"),
    "sites_reload" => cfg!(feature = "sites_reload"),
    _ => false,
  }
}

/// The features `config` and the files under its app directory call for. `draws_cards` is whether the plan holds a card image, which only the build or the loaded plan knows.
pub fn needs(config: &Config, draws_cards: bool) -> Result<Vec<Need>, HostError> {
  let mut needs = Vec::new();
  if let Some(auth) = &config.auth {
    needs.push(Need::new("auth", "[auth]"));
    let providers = auth.provider.iter().map(|provider| (provider.as_str(), "auth.provider".to_owned())).chain(auth.methods.iter().map(|method| (method.provider.as_str(), format!("auth.methods `{}`", method.name))));
    for (provider, at) in providers {
      match provider {
        "oidc" => needs.push(Need::new("oidc", format!("{at}, provider = \"oidc\""))),
        "passkey" => needs.push(Need::new("passkey", format!("{at}, provider = \"passkey\""))),
        _ => {}
      }
    }
  }
  if config.server.tls.is_some() {
    needs.push(Need::new("tls", "[server.tls]"));
  }
  match config.time_zones {
    Some(TimeZones::System) => needs.push(Need::new("tz_system", "[time] zones = \"system\"")),
    Some(TimeZones::Compiled) => needs.push(Need::new("tz_compiled", "[time] zones = \"compiled\"")),
    None => {}
  }
  if let Some(template) = crate::template_files(&config.app)?.first() {
    let at = template.strip_prefix(&config.app).unwrap_or(template).display().to_string();
    needs.push(Need::or("tera", at, Instead::Evaluator));
  }
  if draws_cards {
    needs.push(Need::new("share_cards", "a card image the plan draws"));
  }
  for (name, client) in &config.clients {
    let document = client.document.clone().unwrap_or_else(|| format!("clients/{name}.openapi.json"));
    if document.ends_with(".proto") {
      needs.push(Need::or("grpc", document, Instead::Services));
    } else if document.ends_with(".graphql") {
      needs.push(Need::or("graphql", document, Instead::Services));
    } else if !client.is_mock() {
      needs.push(Need::or("http", format!("{document}, called over HTTP"), Instead::Services));
    }
  }
  Ok(needs)
}

/// What a host built without the features in `missing` says, one line per feature.
pub fn refusal(missing: &[&Need]) -> String {
  let lines: Vec<String> = missing.iter().map(|need| format!("needs the host's {need}")).collect();
  format!("this host is built without features the application calls for:\n  {}", lines.join("\n  "))
}
