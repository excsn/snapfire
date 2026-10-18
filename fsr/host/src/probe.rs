//! What a candidate must answer before a reload swaps it in: the host's own
//! probes over the routes that read nothing of the request, each site's
//! `[site] probe` list and the checks a Rust host registers.

use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use http::StatusCode;
use serde::Deserialize;
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_core::{Params, ValueMap};
use snapfire_fsr_runtime::{Identity, SessionCell};

use crate::render::status_of;
use crate::{fill_pattern, Host, HostError, Incoming, RenderMode, Tables};

/// One path a site asks a shell to render before a candidate holding the
/// site swaps in: `"/billing"` anonymously or `{ path = "/billing/overdue",
/// as = "probe" }` signed in as that subject with no claims.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Probe {
  Path(String),
  As {
    path: String,
    #[serde(rename = "as")]
    subject: String,
  },
}

impl Probe {
  pub fn path(&self) -> &str {
    match self {
      Self::Path(path) | Self::As { path, .. } => path,
    }
  }

  pub fn subject(&self) -> Option<&str> {
    match self {
      Self::Path(_) => None,
      Self::As { subject, .. } => Some(subject),
    }
  }

  fn session(&self) -> SessionCell {
    match self.subject() {
      Some(subject) => SessionCell::new(ValueMap::default(), Some(Identity::new(subject.to_owned(), ValueMap::default()))),
      None => SessionCell::default(),
    }
  }
}

/// A check a Rust host registers with `HostBuilder::reload_check`, run over
/// every candidate after the host's own probes and before the swap. `Err`
/// refuses the candidate with the reason.
pub trait ReloadCheck: Send + Sync {
  fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>>;
}

impl<F> ReloadCheck for F
where
  F: for<'a> Fn(&'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>> + Send + Sync,
{
  fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>> {
    self(candidate)
  }
}

/// The tables a reload would swap in, rendered through the running host's
/// sessions, evaluators and services but never served.
pub struct Candidate<'a> {
  host: &'a Host,
  tables: &'a Tables,
}

impl<'a> Candidate<'a> {
  pub(crate) fn new(host: &'a Host, tables: &'a Tables) -> Self {
    Self { host, tables }
  }

  /// The candidate's boot report.
  pub fn report(&self) -> &crate::HostReport {
    &self.tables.report
  }

  /// `path`, with its query, rendered as a document in the default locale by
  /// a visitor holding `session`: the status `handle` would answer and the
  /// document. A path no route matches is `HostError::NotFound`.
  pub async fn render(&self, path: &str, session: SessionCell) -> Result<(StatusCode, String), HostError> {
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.tables.locales.resolve(bare, None, None);
    let rendered = self.host.render_in(self.tables, &visit, raw_query, RenderMode::Html, Incoming::anonymous(session)).await?;
    let chunks: Vec<String> = rendered.chunks.collect().await;
    Ok((status_of(rendered.failed), chunks.concat()))
  }

  /// `render` signed in as `subject` with no claims.
  pub async fn render_as(&self, path: &str, subject: &str) -> Result<(StatusCode, String), HostError> {
    self.render(path, Probe::As { path: path.to_owned(), subject: subject.to_owned() }.session()).await
  }

  /// The paths the routes that read nothing of the request answer, in the
  /// default locale: every prerenderable pattern, a pattern with a parameter
  /// once per set its `paths` export returns.
  async fn own_paths(&self) -> Result<Vec<String>, HostError> {
    let t = self.tables;
    let locale = t.locales.locale(&t.locales.default);
    let mut out = Vec::new();
    for pattern in t.app.prerenderable.iter().chain(t.app.prerenderable_anonymous.iter()) {
      match t.app.paths.get(pattern) {
        Some(enumerated) => {
          let ctx = self.host.ctx(t, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale.clone());
          let sets = enumerated.paths(&ctx).await.map_err(|e| HostError::Paths(pattern.clone(), e.message))?;
          for set in &sets {
            out.push(fill_pattern(pattern, set)?);
          }
        }
        None if pattern.contains('{') => {}
        None => out.push(pattern.clone()),
      }
    }
    Ok(out)
  }
}

/// A failure the host's own probes refuse a candidate for: any 5xx, which is a
/// backend down, slow or crashed, plus a 408. Any other 4xx is a page's own
/// answer to the request it was given and passes; a configured probe, which
/// names its request, refuses on any of them.
fn refusing(kind: FailureKind) -> bool {
  kind.is_server_error() || kind == FailureKind::RequestTimeout
}

impl Host {
  /// Every probe over `tables`, in order: the warm pass over every memoizable
  /// source, a render of every path the routes that read nothing answer, each
  /// site's `[site] probe` list and then the checks a Rust host registered.
  /// The first refusal is the error, naming the probe.
  pub(crate) async fn probe(&self, tables: &Tables) -> Result<(), HostError> {
    let candidate = Candidate::new(self, tables);
    for name in &tables.app.warmable {
      let id = snapfire_fsr_core::DataSourceId(name.clone());
      let Some(source) = tables.app.runtime.sources.get(&id) else { continue };
      let locale = tables.locales.locale(&tables.locales.default);
      let ctx = self.ctx(tables, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale);
      if let Err(e) = source.load(&ctx).await {
        if refusing(e.kind) {
          return Err(HostError::Probe(format!("source {name}: {} {}", e.kind.as_str(), e.message)));
        }
      }
    }
    for path in candidate.own_paths().await? {
      probe_render(&candidate, None, &path, SessionCell::default(), false).await?;
    }
    for (site, probe) in &tables.probes {
      probe_render(&candidate, Some(site), probe.path(), probe.session(), true).await?;
    }
    for check in &self.reload_checks {
      check.check(&candidate).await.map_err(HostError::Probe)?;
    }
    Ok(())
  }
}

/// One path rendered against the candidate. `strict` is a probe someone
/// named, which must answer the page it asked for; the host's own probes
/// refuse only on a failure the page cannot have meant.
async fn probe_render(candidate: &Candidate<'_>, site: Option<&str>, path: &str, session: SessionCell, strict: bool) -> Result<(), HostError> {
  let who = match (site, session.identity()) {
    (Some(site), Some(identity)) => format!("site {site}, {path} as {}", identity.subject),
    (Some(site), None) => format!("site {site}, {path}"),
    (None, _) => path.to_owned(),
  };
  let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
  let visit = candidate.tables.locales.resolve(bare, None, None);
  let rendered = candidate
    .host
    .render_in(candidate.tables, &visit, raw_query, RenderMode::Html, Incoming::anonymous(session))
    .await
    .map_err(|e| HostError::Probe(format!("{who}: {e}")))?;
  let failed = rendered.failed;
  let _: Vec<String> = rendered.chunks.collect().await;
  match failed {
    Some(kind) if strict || refusing(kind) => Err(HostError::Probe(format!("{who}: the page failed with {}", kind.as_str()))),
    _ => Ok(()),
  }
}
