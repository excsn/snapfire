use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{LocalTransport, Transport};
use fibre::mpsc;

struct Room {
  id: &'static str,
  name: &'static str,
  about: &'static str,
}

const ROOMS: &[Room] = &[
  Room { id: "lobby", name: "Lobby", about: "Anyone, anything" },
  Room { id: "deploys", name: "Deploys", about: "What went out and what came back" },
  Room { id: "kitchen", name: "Kitchen", about: "Whose yoghurt" },
];

#[derive(Clone)]
struct Said {
  id: u64,
  who: String,
  body: String,
  at: String,
}

/// Every room's transcript, and the topics to tell an open page about. The
/// state is a `Mutex` rather than a database because the point of the example
/// is the seam, not the storage.
pub struct Rooms {
  said: Mutex<Vec<(String, Said)>>,
  next: Mutex<u64>,
  topics: Mutex<mpsc::UnboundedSyncSender<String>>,
  /// Taken once, by the task that forwards to the host's `publish`.
  feed: Mutex<Option<mpsc::UnboundedAsyncReceiver<String>>>,
}

impl Rooms {
  fn new() -> Self {
    let (told, hear) = mpsc::unbounded();
    let seed = |id: &str, who: &str, body: &str, at: &str, n: u64| (id.to_owned(), Said { id: n, who: who.to_owned(), body: body.to_owned(), at: at.to_owned() });
    Self {
      said: Mutex::new(vec![
        seed("lobby", "alice", "Morning.", "09:01", 1),
        seed("lobby", "bob", "Morning. Coffee is on.", "09:02", 2),
        seed("deploys", "alice", "3.4.1 is out, nothing on fire.", "08:40", 3),
      ]),
      next: Mutex::new(4),
      topics: Mutex::new(told),
      feed: Mutex::new(Some(hear.to_async())),
    }
  }

  /// What the host forwards to `publish`: one topic per room that changed.
  pub fn changes(&self) -> mpsc::UnboundedAsyncReceiver<String> {
    self.feed.lock().take().expect("the change feed is taken once, by the forwarder")
  }

  fn count(&self, room: &str) -> i64 {
    self.said.lock().iter().filter(|(id, _)| id == room).count() as i64
  }

  fn room(&self, room: &Room) -> Value {
    let mut map = ValueMap::default();
    map.insert("id".to_owned(), Value::str(room.id));
    map.insert("name".to_owned(), Value::str(room.name));
    map.insert("about".to_owned(), Value::str(room.about));
    map.insert("messages".to_owned(), Value::int(self.count(room.id)));
    Value::Map(map)
  }

  fn message(said: &Said) -> Value {
    let mut map = ValueMap::default();
    map.insert("id".to_owned(), Value::Str(said.id.to_string()));
    map.insert("who".to_owned(), Value::Str(said.who.clone()));
    map.insert("body".to_owned(), Value::Str(said.body.clone()));
    map.insert("at".to_owned(), Value::Str(said.at.clone()));
    Value::Map(map)
  }

  fn transcript(&self, id: &str) -> Option<Value> {
    let room = ROOMS.iter().find(|room| room.id == id)?;
    let messages: Vec<Value> = self.said.lock().iter().filter(|(at, _)| at == id).map(|(_, said)| Self::message(said)).collect();
    let mut map = ValueMap::default();
    map.insert("room".to_owned(), self.room(room));
    map.insert("messages".to_owned(), Value::seq(messages));
    Some(Value::Map(map))
  }

  /// Keeps a message and tells everyone in that room. `at` is the wall clock,
  /// since a chat is the one place a reader wants the real one.
  fn keep(&self, room: &str, who: &str, body: &str) -> Value {
    let said = {
      let mut next = self.next.lock();
      let said = Said { id: *next, who: who.to_owned(), body: body.to_owned(), at: now() };
      *next += 1;
      said
    };
    self.said.lock().push((room.to_owned(), said.clone()));
    let _ = self.topics.lock().send(format!("room/{room}"));
    Self::message(&said)
  }
}

fn now() -> String {
  let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
  format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}

/// The rooms service in process, and the handle the host publishes from.
pub fn rooms() -> (Arc<dyn Transport>, Arc<Rooms>) {
  let state = Arc::new(Rooms::new());
  let (listing, reading, writing) = (state.clone(), state.clone(), state.clone());
  let transport = LocalTransport::new()
    .method("rooms.listRooms", move |_| {
      let rooms = Value::Seq(ROOMS.iter().map(|room| listing.room(room)).collect());
      async move { Ok(rooms) }
    })
    .method("rooms.getRoom", move |call| {
      let id = string(&call.args, "id");
      let transcript = reading.transcript(&id);
      async move {
        transcript.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "rooms", "getRoom", format!("no room `{id}`")))
      }
    })
    .method("rooms.postMessage", move |call| {
      let (id, who, body) = (string(&call.args, "id"), string(&call.args, "who"), string(&call.args, "body"));
      let kept = writing.keep(&id, &who, &body);
      async move { Ok(kept) }
    });
  (Arc::new(transport), state)
}

fn string(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(text)) => text.clone(),
    _ => String::new(),
  }
}
