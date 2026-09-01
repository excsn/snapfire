use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use snapfire_fsr_core::Node;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::{
  assemble, html_stream, wire_stream, ActionError, AssembleError, Matcher, RequestCtx, Resolver,
  SessionCell,
};

use crate::server::AppCore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
  Html,
  Payload,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
  #[error("no route matches `{0}`")]
  NotFound(String),
  #[error(transparent)]
  Assemble(#[from] AssembleError),
}

fn head() -> Node {
  let mut head = String::from("<title>Shopping</title><meta charset=\"utf-8\">");
  head.push_str("<script type=\"importmap\">");
  head.push_str(include_str!("../../app/importmap.json"));
  head.push_str("</script><script type=\"module\" src=\"/static/js/app/main.js\"></script>");
  Node::raw(head)
}

pub async fn respond_with(
  app: &AppCore,
  path: &str,
  mode: RenderMode,
  session: SessionCell,
) -> Result<BoxStream<'static, String>, AppError> {
  let matched = app
    .matcher
    .match_path(path)
    .ok_or_else(|| AppError::NotFound(path.to_owned()))?;
  let plan = app
    .resolver
    .resolve(matched.entry, &matched.params)
    .ok_or_else(|| AppError::NotFound(path.to_owned()))?;

  let ctx = RequestCtx {
    params: matched.params,
    session,
    csrf: None,
    services: app.services.bind_anonymous(),
  };
  let assembly = assemble(&app.runtime, &plan, &ctx, &head()).await?;
  Ok(match mode {
    RenderMode::Html => Box::pin(html_stream(assembly)),
    RenderMode::Payload => Box::pin(wire_stream(assembly)),
  })
}

/// The action edge. An action is a stable id, so the browser holds a
/// reference rather than a URL.
pub async fn call_action(
  app: &AppCore,
  id: &str,
  session: SessionCell,
  input: Value,
) -> Result<Value, ActionError> {
  let ctx = RequestCtx {
    params: Default::default(),
    session,
    csrf: None,
    services: app.services.bind_anonymous(),
  };
  app.actions.dispatch(id, ctx, input).await
}

pub async fn render(app: &AppCore, path: &str, mode: RenderMode) -> Result<String, AppError> {
  let chunks: Vec<String> = respond_with(app, path, mode, SessionCell::default()).await?.collect().await;
  Ok(chunks.concat())
}
