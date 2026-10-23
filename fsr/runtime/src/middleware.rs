//! Middleware's `next`: the rest of the chain runs for the request the middleware names and its response comes back with the body held aside, so the middleware answers with it, changed or not; it may answer with a response of its own instead.

use std::future::Future;
use std::sync::Arc;

use futures_channel::{mpsc, oneshot};
use futures_util::future::{BoxFuture, Either, select};
use futures_util::StreamExt;
use snapfire_fsr_core::Value;

use crate::{ActionError, FailureKind, HttpResponse, RequestCtx};

/// The tag of the `Value::Variant` `new Request(url)` makes; its payload is an object holding `url`.
pub const REQUEST_TAG: &str = "$request";

type NextFn = dyn Fn(Value) -> BoxFuture<'static, Result<Value, ActionError>> + Send + Sync;

/// `next` in a middleware's context: hands the request on and answers the route's response.
#[derive(Clone)]
pub struct NextHandle(Arc<NextFn>);

impl NextHandle {
  pub fn new(next: impl Fn(Value) -> BoxFuture<'static, Result<Value, ActionError>> + Send + Sync + 'static) -> Self {
    Self(Arc::new(next))
  }

  /// Runs the rest of the chain for `request`: nothing for the request as it came, a path, a `Request` or the middleware's own `request`.
  pub fn call(&self, request: Value) -> BoxFuture<'static, Result<Value, ActionError>> {
    (self.0)(request)
  }
}

/// Where `next` sends a request: a path on this origin and its query.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
  pub path: String,
  pub query: String,
}

impl Target {
  /// The target `request` names, the request as it came for nothing. An absolute URL keeps its path and query.
  pub fn of(request: &Value, path: &str, query: &str) -> Result<Self, ActionError> {
    let url = match request {
      Value::Null | Value::Undefined => return Ok(Self { path: path.to_owned(), query: query.to_owned() }),
      Value::Str(url) => url.to_string(),
      Value::Variant { tag, payload: Some(payload) } if tag == REQUEST_TAG => match &**payload {
        Value::Map(fields) => match fields.get("url") {
          Some(Value::Str(url)) => url.to_string(),
          _ => return Err(refused("a `Request` without its url")),
        },
        _ => return Err(refused("a `Request` without its url")),
      },
      Value::Map(fields) => match fields.get("path") {
        Some(Value::Str(path)) => path.to_string(),
        _ => return Err(refused("`next` takes a path, a `Request` or the middleware's `request`")),
      },
      _ => return Err(refused("`next` takes a path, a `Request` or the middleware's `request`")),
    };
    let local = match url.split_once("://") {
      Some((_, rest)) => rest.find('/').map_or("/".to_owned(), |at| rest[at..].to_owned()),
      None => url,
    };
    if !local.starts_with('/') {
      return Err(refused(&format!("`next` runs a path on this origin, not `{local}`")));
    }
    let (path, query) = local.split_once('?').unwrap_or((local.as_str(), ""));
    Ok(Self { path: path.to_owned(), query: query.to_owned() })
  }
}

fn refused(message: &str) -> ActionError {
  ActionError::new(FailureKind::Internal, message.to_owned())
}

/// How a middleware answered.
pub enum Passed<B> {
  /// Nothing, without calling `next`: the request goes on as it came.
  Continue,
  /// A response of its own.
  Answer(HttpResponse),
  /// The route's response, with the status and headers the middleware left on it and the body the route answered.
  Route(HttpResponse, B),
}

/// Runs a middleware, `run`, with `next` in its context, serving each `next` call with `serve` while the middleware waits on it. `query` is the request's query as it came; its path is `ctx.path`.
pub async fn drive_middleware<B, Run, Serve, Fut>(mut ctx: RequestCtx, query: &str, run: Run, mut serve: Serve) -> Result<Passed<B>, ActionError>
where
  Run: FnOnce(RequestCtx) -> BoxFuture<'static, Result<Value, ActionError>>,
  Serve: FnMut(Target) -> Fut,
  Fut: Future<Output = Result<(HttpResponse, B), ActionError>>,
{
  let (tx, mut asked) = mpsc::unbounded::<(Value, oneshot::Sender<Result<Value, ActionError>>)>();
  ctx.next = Some(NextHandle::new(move |request| {
    let tx = tx.clone();
    Box::pin(async move {
      let (reply, answer) = oneshot::channel();
      tx.unbounded_send((request, reply)).map_err(|_| refused("`next` called after the middleware answered"))?;
      answer.await.map_err(|_| refused("the route did not answer `next`"))?
    })
  }));
  let path = ctx.path.clone();
  let mut running = run(ctx);
  let mut served: Vec<(HttpResponse, Option<B>)> = Vec::new();
  let value = loop {
    match select(running, asked.next()).await {
      Either::Left((done, _)) => break done?,
      Either::Right((Some((request, reply)), still)) => {
        running = still;
        let answered = match Target::of(&request, &path, query) {
          Ok(target) => serve(target).await,
          Err(e) => Err(e),
        };
        let answered = answered.map(|(mut head, body)| {
          head.downstream = Some(served.len());
          served.push((head.clone(), Some(body)));
          head.to_value()
        });
        let _ = reply.send(answered);
      }
      Either::Right((None, still)) => running = still,
    }
  };
  match HttpResponse::from_value(&value) {
    None if matches!(value, Value::Null | Value::Undefined) => match served.pop() {
      Some((mut head, Some(body))) => {
        head.downstream = None;
        Ok(Passed::Route(head, body))
      }
      _ => Ok(Passed::Continue),
    },
    None => Err(refused("middleware answers a `Response` or nothing")),
    Some(Err(message)) => Err(refused(&message)),
    Some(Ok(mut response)) => match response.downstream.take() {
      None => Ok(Passed::Answer(response)),
      Some(at) => match served.get_mut(at).and_then(|(_, body)| body.take()) {
        Some(body) => Ok(Passed::Route(response, body)),
        None => Err(refused("middleware answered the route's response twice")),
      },
    },
  }
}
