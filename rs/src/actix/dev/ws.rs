use crate::core::reload::ReloadMessage;
use actix_web::{HttpRequest, HttpResponse};
use actix_ws::{AggregatedMessage, MessageStream, Session};
use futures_util::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::interval;

/// How often heartbeat pings are sent to the client.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long to wait for a pong response before timing out.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// The main entry point function for handling a new WebSocket connection request.
/// This function is the Actix handler.
pub(crate) async fn websocket_handler(
  req: HttpRequest,
  body: actix_web::web::Payload,
  broadcaster: broadcast::Sender<ReloadMessage>,
) -> Result<HttpResponse, actix_web::Error> {
  log::info!("New WebSocket connection request");

  let (response, session, msg_stream) = actix_ws::handle(&req, body)?;

  actix_web::rt::spawn(handle_connection(session, msg_stream, broadcaster.subscribe()));

  Ok(response)
}

/// Handles the lifecycle of a single WebSocket connection.
async fn handle_connection(
  mut session: Session,
  msg_stream: MessageStream,
  mut reloader_rx: broadcast::Receiver<ReloadMessage>,
) {
  let mut last_heartbeat = Instant::now();
  let mut interval = interval(HEARTBEAT_INTERVAL);

  let mut msg_stream = msg_stream.aggregate_continuations();
  let close_reason = loop {
    tokio::select! {
      _ = interval.tick() => {
        if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
          log::info!("WebSocket client heartbeat failed, disconnecting!");
          break None;
        }
        if session.ping(b"").await.is_err() {
          break None;
        };
      }

      Some(Ok(msg)) = msg_stream.next() => {
        match msg {
          AggregatedMessage::Ping(bytes) => {
            last_heartbeat = Instant::now();
            if session.pong(&bytes).await.is_err() {
              break None;
            }
          }
          AggregatedMessage::Pong(_) => {
              last_heartbeat = Instant::now();
          }
          AggregatedMessage::Close(reason) => {
            break reason;
          }
          AggregatedMessage::Text(_) | AggregatedMessage::Binary(_) => {
              // We don't process incoming text/binary messages, just ignore them.
          }
        }
      }

      Ok(reload_msg) = reloader_rx.recv() => {
        let message_text = match reload_msg {
          ReloadMessage::Reload => "reload",
          ReloadMessage::ReloadCss => "reload-css",
        };
        log::debug!("Broadcasting WebSocket message: {}", message_text);

        if session.text(message_text).await.is_err() {
          break None;
        }
      }

      // The client stream has closed
      else => break None,
    }
  };

  let _ = session.close(close_reason).await;
}
