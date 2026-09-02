use std::sync::Arc;

use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use snapfire_fsr_core::{Node, Value};
use snapfire_fsr_runtime::{
  assemble, html_stream, wire_stream, ActionError, AssembleError, Matcher, RequestCtx, Resolver,
  SessionCell,
};
use snapfire_fsr_service::{Credentials, NoCredentials};

use crate::AppCore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
  Html,
  Payload,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
  #[error("no route matches `{0}`")]
  NotFound(String),
  #[error("unsupported payload encoding `{0}`")]
  UnsupportedEncoding(String),
  #[error(transparent)]
  Assemble(#[from] AssembleError),
}

/// The response's V row names what it got; a request may name what it wants.
/// Only the JSON pair exists so far.
pub fn negotiate_encoding(requested: Option<&str>) -> Result<&'static str, AppError> {
  match requested {
    None | Some("json") => Ok("json"),
    Some(other) => Err(AppError::UnsupportedEncoding(other.to_owned())),
  }
}

fn escape_title(raw: &str) -> String {
  raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn head_node(title: &str) -> Node {
  let mut head = format!("<title>{}</title>", escape_title(title));
  head.push_str("<script type=\"importmap\">");
  head.push_str(include_str!("../js/importmap.json"));
  head.push_str("</script><script type=\"module\" src=\"/static/js/app/main.js\"></script>");
  Node::raw(head)
}

async fn compute_title(app: &AppCore, entry: snapfire_fsr_runtime::EntryId, ctx: &RequestCtx) -> String {
  let fallback = "Snapfire FSR".to_owned();
  let Some(source_id) = crate::routes::metadata_source(entry) else { return fallback };
  let Some(source) = app.runtime.sources.get(&source_id) else { return fallback };
  match source.load(ctx).await {
    Ok(data) => match data.get("title") {
      Some(Value::Str(title)) => title.clone(),
      _ => fallback,
    },
    Err(_) => fallback,
  }
}

/// What the HTTP edge knows before a route is matched. `credentials` is the
/// request's token custody, which the service layer reads and application code
/// cannot.
pub struct Incoming {
  pub session: SessionCell,
  pub csrf: Option<String>,
  pub credentials: Arc<dyn Credentials>,
}

impl Default for Incoming {
  fn default() -> Self {
    Self { session: SessionCell::default(), csrf: None, credentials: Arc::new(NoCredentials) }
  }
}

impl Incoming {
  pub fn new(session: SessionCell, csrf: Option<String>, credentials: Arc<dyn Credentials>) -> Self {
    Self { session, csrf, credentials }
  }
}

/// The response as a stream of chunks: one chunk for a fully-eager page, more
/// as deferred slots resolve.
pub async fn respond_with(
  app: &AppCore,
  path: &str,
  mode: RenderMode,
  incoming: Incoming,
) -> Result<BoxStream<'static, String>, AppError> {
  let matched = app
    .matcher
    .match_path(path)
    .ok_or_else(|| AppError::NotFound(path.to_owned()))?;
  let plan = app
    .resolver
    .resolve(matched.entry, &matched.params)
    .ok_or_else(|| AppError::NotFound(path.to_owned()))?;
  app.renders.next();
  // A visit is a page the browser loaded. A payload request is the same page
  // navigating, so it renders without counting.
  if mode == RenderMode::Html {
    let visits = match incoming.session.get("visits") {
      Some(Value::Int(n)) => n + 1,
      _ => 1,
    };
    incoming.session.insert("visits", Value::Int(visits));
  }
  let services = app.services.bind(incoming.session.identity(), incoming.credentials);
  let ctx = RequestCtx {
    params: matched.params,
    query: Default::default(),
    session: incoming.session,
    csrf: incoming.csrf,
    services,
  };
  let title = compute_title(app, matched.entry, &ctx).await;
  let assembly = assemble(&app.runtime, &plan, &ctx, &head_node(&title)).await?;
  Ok(match mode {
    RenderMode::Html => Box::pin(html_stream(assembly)),
    RenderMode::Payload => Box::pin(wire_stream(assembly)),
  })
}

pub async fn respond(
  app: &AppCore,
  path: &str,
  mode: RenderMode,
) -> Result<BoxStream<'static, String>, AppError> {
  respond_with(app, path, mode, Incoming::default()).await
}

pub async fn render(app: &AppCore, path: &str, mode: RenderMode) -> Result<String, AppError> {
  let chunks: Vec<String> = respond(app, path, mode).await?.collect().await;
  Ok(chunks.concat())
}

pub async fn call_action(app: &AppCore, id: &str, ctx: RequestCtx, input: Value) -> Result<Value, ActionError> {
  app.actions.dispatch(id, ctx, input).await
}
