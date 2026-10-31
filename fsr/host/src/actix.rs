//! The one framework that needs a shim: actix has its own request and
//! response types. This maps them onto the `http` types the host speaks.

use std::sync::Arc;

use actix_web::web::{Bytes, Data};
use actix_web::{App, FromRequest, HttpRequest, HttpResponse, HttpServer, web};
use futures_util::TryStreamExt;
use http::Request;
use http_body_util::BodyExt;

use crate::Host;

/// One actix handler over the host, for embedding in an existing actix `App`
/// as its default service. `/_sf/socket` upgrades here, since the upgrade the
/// host does itself needs hyper's connection.
pub async fn handle(req: HttpRequest, host: Data<Arc<Host>>, payload: web::Payload) -> HttpResponse {
  #[cfg(feature = "ws")]
  let binding = (req.path() == "/_sf/socket" && req.headers().contains_key("sec-websocket-key")).then(crate::socket::BindingUpgrade::default);
  #[cfg(not(feature = "ws"))]
  let binding: Option<()> = None;
  let (body, payload) = if binding.is_some() {
    (Bytes::new(), Some(payload))
  } else {
    match Bytes::from_request(&req, &mut payload.into_inner()).await {
      Ok(body) => (body, None),
      Err(e) => return e.error_response(),
    }
  };
  let mut builder = Request::builder()
    .method(req.method().as_str())
    .uri(req.uri().to_string());
  for (name, value) in req.headers() {
    builder = builder.header(name.as_str(), value.as_bytes());
  }
  #[cfg(feature = "ws")]
  if let Some(binding) = &binding {
    builder = builder.extension(binding.clone());
  }
  let request = match builder.body(bytes::Bytes::copy_from_slice(&body)) {
    Ok(request) => request,
    Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
  };

  let answered = host.handle(request).await;
  #[cfg(feature = "ws")]
  if let (Some(accepted), Some(payload)) = (binding.and_then(|binding| binding.0.lock().take()), payload) {
    return match actix_ws::handle(&req, payload) {
      Ok((response, session, messages)) => {
        actix_web::rt::spawn(serve_socket(session, messages, accepted));
        response
      }
      Err(e) => e.error_response(),
    };
  }
  to_actix(answered)
}

/// One upgraded connection, as `socket::serve` serves one over hyper's.
#[cfg(feature = "ws")]
async fn serve_socket(mut session: actix_ws::Session, mut messages: actix_ws::MessageStream, accepted: crate::socket::Accepted) {
  use crate::socket::{On, Who, decode};
  use futures_util::StreamExt;

  let crate::socket::Accepted { who, sockets, handler } = accepted;
  let (id, mut inbox) = sockets.join(&who.topic);
  let who = Who { connection: id, ..who };
  sockets.deliver(&who.topic, &handler(&who, On::Joined), id);

  loop {
    tokio::select! {
      frame = inbox.recv() => match frame {
        Ok(frame) => {
          if session.text(frame.text().to_string()).await.is_err() {
            break;
          }
        }
        Err(_) => break,
      },
      message = messages.next() => match message {
        Some(Ok(actix_ws::Message::Text(text))) => {
          if let Some(row) = decode(&text) {
            sockets.deliver(&who.topic, &handler(&who, On::Said(row)), id);
          }
        }
        Some(Ok(actix_ws::Message::Ping(bytes))) => {
          if session.pong(&bytes).await.is_err() {
            break;
          }
        }
        Some(Ok(actix_ws::Message::Close(_))) | None => break,
        Some(Ok(_)) => {}
        Some(Err(e)) => {
          tracing::debug!(target: "fsr::host", error = %e, "socket ended");
          break;
        }
      },
    }
  }

  sockets.leave(&who.topic, id);
  sockets.deliver(&who.topic, &handler(&who, On::Left), id);
  let _ = session.close(None).await;
}

/// The host's response as actix's, headers as the host wrote them.
fn to_actix(response: http::Response<crate::Body>) -> HttpResponse {
  let (parts, body) = response.into_parts();
  let mut out = HttpResponse::build(
    actix_web::http::StatusCode::from_u16(parts.status.as_u16())
      .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
  );
  for (name, value) in &parts.headers {
    if let Ok(name) = actix_web::http::header::HeaderName::from_bytes(name.as_str().as_bytes()) {
      if let Ok(value) = actix_web::http::header::HeaderValue::from_bytes(value.as_bytes()) {
        out.append_header((name, value));
      }
    }
  }
  if http_body::Body::size_hint(&body).exact() == Some(0) {
    return out.finish();
  }
  let stream = body
    .into_data_stream()
    .map_ok(|chunk| Bytes::copy_from_slice(&chunk))
    .map_err(actix_web::error::ErrorInternalServerError);
  out.streaming(stream)
}

/// Serves the host with actix on `addr` until SIGTERM or SIGINT, which turns
/// `server.ready` to 503 and gives open requests `server.shutdown_grace`.
pub async fn serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()> {
  let grace = host.shutdown_grace.as_secs().max(1);
  let app_host = host.clone();
  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(app_host.clone()))
      .default_service(web::to(handle))
  })
  .disable_signals()
  .shutdown_timeout(grace)
  .bind(addr)?
  .run();
  let handle = server.handle();
  actix_web::rt::spawn(async move {
    crate::serve::shutdown_signal().await;
    host.begin_drain();
    handle.stop(true).await;
  });
  server.await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_response_with_no_body_and_no_type_gains_no_type() {
    let response = http::Response::builder()
      .status(308)
      .header("location", "/agents/3")
      .body(http_body_util::Full::new(bytes::Bytes::new()).map_err(|never| match never {}).boxed_unsync())
      .unwrap();
    let answered = to_actix(response);
    assert_eq!(answered.status().as_u16(), 308);
    assert_eq!(answered.headers().get("location").map(|v| v.as_bytes()), Some(&b"/agents/3"[..]));
    assert!(answered.headers().get("content-type").is_none(), "{:?}", answered.headers());
  }
}
