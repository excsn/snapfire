//! `[ws]`: `/_sf/socket`, the direction `/_sf/live` does not go. A page opens
//! one per topic, sends into it, and what the application makes of what it
//! sent is broadcast to everyone else on that topic as store rows. Behind the
//! `ws` feature.

#![cfg(feature = "ws")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::{Identity, SessionCell};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// One store row on the wire: the key an island reads and the value it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
  pub key: String,
  pub value: Value,
}

impl Row {
  pub fn new(key: impl Into<String>, value: Value) -> Self {
    Self { key: key.into(), value }
  }
}

/// What happened on a topic, for the application to answer. `Joined` and
/// `Left` are how presence is written without the framework deciding what
/// presence means; `Said` is one row a page sent.
#[derive(Debug, Clone)]
pub enum On {
  Joined,
  Said(Row),
  Left,
}

/// Who it happened to. The session is the one the socket's cookie named when
/// it opened, read as it was then.
pub struct Who {
  pub topic: String,
  pub session: SessionCell,
  pub identity: Option<Identity>,
  /// This connection, distinct from every other on the same topic, which is
  /// what tells two windows of one session apart.
  pub connection: u64,
}

/// The rows to send back, and to whom. `Everyone` includes the sender, which
/// is what a transcript wants; `Others` is what a typing indicator wants.
#[derive(Debug, Clone, Default)]
pub struct Reply {
  pub everyone: Vec<Row>,
  pub others: Vec<Row>,
  pub sender: Vec<Row>,
}

impl Reply {
  pub fn everyone(rows: impl IntoIterator<Item = Row>) -> Self {
    Self { everyone: rows.into_iter().collect(), ..Self::default() }
  }

  pub fn others(rows: impl IntoIterator<Item = Row>) -> Self {
    Self { others: rows.into_iter().collect(), ..Self::default() }
  }

  pub fn sender(rows: impl IntoIterator<Item = Row>) -> Self {
    Self { sender: rows.into_iter().collect(), ..Self::default() }
  }
}

/// What an application makes of what a page sent. Returning `Reply::default()`
/// drops it, which is what an unrecognised key deserves.
pub type SocketHandler = Arc<dyn Fn(&Who, On) -> Reply + Send + Sync>;

/// Every open socket, by topic. A connection is dropped from its topic when
/// its writer half goes, which happens when the browser closes the socket or
/// the connection breaks.
#[derive(Default)]
pub struct Sockets {
  open: parking_lot::Mutex<HashMap<String, Vec<(u64, mpsc::UnboundedSender<Vec<Row>>)>>>,
  next: AtomicU64,
}

impl Sockets {
  pub fn new() -> Self {
    Self::default()
  }

  fn join(&self, topic: &str) -> (u64, mpsc::UnboundedReceiver<Vec<Row>>) {
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::unbounded_channel();
    self.open.lock().entry(topic.to_owned()).or_default().push((id, tx));
    (id, rx)
  }

  fn leave(&self, topic: &str, id: u64) {
    let mut open = self.open.lock();
    if let Some(peers) = open.get_mut(topic) {
      peers.retain(|(peer, _)| *peer != id);
      if peers.is_empty() {
        open.remove(topic);
      }
    }
  }

  /// How many sockets are open on a topic, which is what a presence count is
  /// before the application decides what to call it.
  pub fn on(&self, topic: &str) -> usize {
    self.open.lock().get(topic).map(Vec::len).unwrap_or(0)
  }

  fn send(&self, topic: &str, rows: &[Row], to: Reach, from: u64) {
    if rows.is_empty() {
      return;
    }
    let open = self.open.lock();
    let Some(peers) = open.get(topic) else { return };
    for (id, tx) in peers {
      let wanted = match to {
        Reach::Everyone => true,
        Reach::Others => *id != from,
        Reach::Sender => *id == from,
      };
      if wanted {
        let _ = tx.send(rows.to_vec());
      }
    }
  }

  fn deliver(&self, topic: &str, reply: &Reply, from: u64) {
    self.send(topic, &reply.everyone, Reach::Everyone, from);
    self.send(topic, &reply.others, Reach::Others, from);
    self.send(topic, &reply.sender, Reach::Sender, from);
  }

  /// Sends rows to a topic from outside any connection, which is how a
  /// backend pushes into a wave nobody typed into.
  pub fn push(&self, topic: &str, rows: impl IntoIterator<Item = Row>) {
    let rows: Vec<Row> = rows.into_iter().collect();
    self.send(topic, &rows, Reach::Everyone, u64::MAX);
  }
}

#[derive(Clone, Copy)]
enum Reach {
  Everyone,
  Others,
  Sender,
}

fn encode(rows: &[Row]) -> String {
  let rows: Vec<serde_json::Value> = rows
    .iter()
    .map(|row| serde_json::json!({ "key": row.key, "value": snapfire_fsr_payload::value_to_json(&row.value) }))
    .collect();
  serde_json::json!({ "rows": rows }).to_string()
}

fn decode(text: &str) -> Option<Row> {
  let json: serde_json::Value = serde_json::from_str(text).ok()?;
  let key = json.get("key")?.as_str()?.to_owned();
  let value = snapfire_fsr_payload::json_to_value(json.get("value")?).ok()?;
  Some(Row { key, value })
}

/// Serves one upgraded connection: the application is told when it joined,
/// once per row it sends and when it left, and whatever it answers goes out
/// to the topic.
pub async fn serve<S>(stream: S, sockets: Arc<Sockets>, handler: SocketHandler, who: Who)
where
  S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
    + futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
    + Unpin,
{
  let (id, mut inbox) = sockets.join(&who.topic);
  let who = Who { connection: id, ..who };
  sockets.deliver(&who.topic, &handler(&who, On::Joined), id);

  let (mut writer, mut reader) = stream.split();
  loop {
    tokio::select! {
      rows = inbox.recv() => match rows {
        Some(rows) => {
          if writer.send(Message::Text(encode(&rows).into())).await.is_err() {
            break;
          }
        }
        None => break,
      },
      frame = reader.next() => match frame {
        Some(Ok(Message::Text(text))) => {
          if let Some(row) = decode(&text) {
            sockets.deliver(&who.topic, &handler(&who, On::Said(row)), id);
          }
        }
        Some(Ok(Message::Close(_))) | None => break,
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
}
