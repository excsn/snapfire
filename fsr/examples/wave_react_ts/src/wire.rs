use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use async_trait::async_trait;
use plaza::session::{session_channel, PresenceEvent, SessionReceiver, SessionSender};
use plaza::{MessageTarget, PlazaError, Session, SessionMessage};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::socket::{On, Reply, Row, Sockets, Who};

use crate::field::{Conn, Op};

/// Plaza's transport, over the socket the host already holds. Nothing here
/// decides anything: it turns a connection into an agent, a row into an op
/// and a view back into rows.
pub struct Wire {
  sockets: Arc<Sockets>,
  /// Which topic each connection is on, since a row goes out by topic.
  topics: Mutex<BTreeMap<Conn, String>>,
  incoming: SessionSender<SessionMessage<Op, Conn>>,
  presence: SessionSender<PresenceEvent<Conn>>,
  taken: Mutex<(Option<SessionReceiver<SessionMessage<Op, Conn>>>, Option<SessionReceiver<PresenceEvent<Conn>>>)>,
}

impl Wire {
  pub fn new(sockets: Arc<Sockets>) -> Arc<Self> {
    let (incoming, inbox) = session_channel(256);
    let (presence, comings) = session_channel(256);
    Arc::new(Self { sockets, topics: Mutex::new(BTreeMap::new()), incoming, presence, taken: Mutex::new((Some(inbox), Some(comings))) })
  }

  /// The host's socket handler: a join, a row or a leave becomes something
  /// the controller reads. Nothing is answered here, because everything a
  /// page is told comes back as a snapshot.
  pub fn on(&self, who: &Who, event: On) -> Reply {
    match event {
      On::Joined => {
        self.topics.lock().insert(who.connection, who.topic.clone());
        let _ = self.presence.try_send(PresenceEvent::Joined { agent: plaza::Agent::Human(who.connection), conn_id: who.connection });
        if let Some(wave) = who.topic.strip_prefix("wave/") {
          let name = match who.session.get("name") {
            Some(Value::Str(name)) => name,
            _ => String::new(),
          };
          self.submit(who.connection, Op::Watch { wave: wave.to_owned(), name });
        }
      }
      On::Said(row) if row.key == "typing" => {
        let (parent, body) = match &row.value {
          Value::Map(map) => (text(map, "parent"), text(map, "body")),
          _ => (String::new(), String::new()),
        };
        self.submit(who.connection, Op::Typing { parent, body });
      }
      On::Said(row) if row.key == "named" => {
        let name = match &row.value {
          Value::Map(map) => text(map, "name"),
          _ => String::new(),
        };
        if let Some(wave) = who.topic.strip_prefix("wave/") {
          self.submit(who.connection, Op::Watch { wave: wave.to_owned(), name });
        }
      }
      On::Said(row) if row.key == "open" => self.submit(who.connection, Op::Open { blip: blip_of(&row) }),
      On::Said(row) if row.key == "rewriting" => {
        let body = match &row.value {
          Value::Map(map) => text(map, "body"),
          _ => String::new(),
        };
        self.submit(who.connection, Op::Rewriting { blip: blip_of(&row), body });
      }
      On::Said(row) if row.key == "close" => self.submit(who.connection, Op::Close { blip: blip_of(&row) }),
      On::Said(_) => {}
      On::Left => {
        self.topics.lock().remove(&who.connection);
        let _ = self.presence.try_send(PresenceEvent::Left { agent_id: who.connection, conn_id: who.connection });
      }
    }
    Reply::default()
  }

  fn submit(&self, conn: Conn, op: Op) {
    let _ = self.incoming.try_send(SessionMessage::new(plaza::Agent::Human(conn), vec![op]));
  }

  /// Everyone the target names, as connections on the topics they are on.
  fn addressed(&self, target: &MessageTarget<Conn>) -> Vec<Conn> {
    let topics = self.topics.lock();
    let all = || topics.keys().copied().collect::<Vec<_>>();
    match target {
      MessageTarget::All => all(),
      MessageTarget::Agent(id) => vec![*id],
      MessageTarget::Agents(ids) => ids.clone(),
      MessageTarget::AllExcept(id) => all().into_iter().filter(|conn| conn != id).collect(),
      MessageTarget::AllExceptThese(ids) => all().into_iter().filter(|conn| !ids.contains(conn)).collect(),
    }
  }
}

#[async_trait]
impl Session<Op, Conn> for Wire {
  async fn send_message(&self, target: MessageTarget<Conn>, msg: SessionMessage<Op, Conn>) -> Result<(), PlazaError<Conn>> {
    let rows: Vec<Row> = msg.ops.iter().flat_map(rows_of).collect();
    if rows.is_empty() {
      return Ok(());
    }
    for conn in self.addressed(&target) {
      let Some(topic) = self.topics.lock().get(&conn).cloned() else { continue };
      self.sockets.push_to(&topic, conn, rows.clone());
    }
    Ok(())
  }

  fn subscribe_to_incoming_messages(&self) -> SessionReceiver<SessionMessage<Op, Conn>> {
    self.taken.lock().0.take().expect("the inbound stream is taken once, by the controller")
  }

  fn on_presence_change(&self) -> SessionReceiver<PresenceEvent<Conn>> {
    self.taken.lock().1.take().expect("the presence stream is taken once, by the controller")
  }
}

/// A view becomes the three rows the page reads; nothing else goes out. The
/// keys are the ones the islands already read, so the browser never learns
/// that a controller now owns what it is being told.
fn rows_of(op: &Op) -> Vec<Row> {
  match op {
    Op::View(view) => {
      let here = Value::Seq(view.here.iter().map(|who| Value::Str(who.clone())).collect());
      let drafts = Value::Seq(
        view
          .drafts
          .iter()
          .map(|draft| {
            let mut map = ValueMap::default();
            map.insert("who".to_owned(), Value::Str(draft.who.clone()));
            map.insert("parent".to_owned(), Value::Str(draft.parent.clone()));
            map.insert("body".to_owned(), Value::Str(draft.body.clone()));
            Value::Map(map)
          })
          .collect(),
      );
      let edits = Value::Seq(
        view
          .edits
          .iter()
          .map(|edit| {
            let mut map = ValueMap::default();
            map.insert("blip".to_owned(), Value::Str(edit.blip.clone()));
            map.insert("who".to_owned(), Value::Str(edit.who.clone()));
            map.insert("body".to_owned(), Value::Str(edit.body.clone()));
            Value::Map(map)
          })
          .collect(),
      );
      vec![Row::new("wave/here", here), Row::new("wave/drafts", drafts), Row::new("wave/edits", edits)]
    }
    _ => Vec::new(),
  }
}

/// The blip a row names, which every rewriting row carries.
fn blip_of(row: &Row) -> String {
  match &row.value {
    Value::Map(map) => text(map, "blip"),
    _ => String::new(),
  }
}

fn text(map: &ValueMap, key: &str) -> String {
  match map.get(key) {
    Some(Value::Str(text)) => text.clone(),
    _ => String::new(),
  }
}
