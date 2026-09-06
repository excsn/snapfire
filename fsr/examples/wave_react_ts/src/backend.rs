use std::sync::Arc;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{LocalTransport, Transport};
use tokio::sync::broadcast;

#[derive(Clone)]
struct Blip {
  id: u64,
  parent: String,
  who: String,
  body: String,
  at: String,
}

struct Wave {
  id: &'static str,
  title: &'static str,
  participants: Vec<String>,
  blips: Vec<Blip>,
}

/// Every wave and its blips, and the topics to tell an open page about. A
/// blip is durable and goes through a loader; a keystroke is not and goes
/// through the socket, which is the whole point of the example.
pub struct Waves {
  waves: Mutex<Vec<Wave>>,
  next: Mutex<u64>,
  topics: broadcast::Sender<String>,
}

fn blip(id: u64, parent: &str, who: &str, body: &str, at: &str) -> Blip {
  Blip { id, parent: parent.to_owned(), who: who.to_owned(), body: body.to_owned(), at: at.to_owned() }
}

impl Waves {
  fn new() -> Self {
    Self {
      waves: Mutex::new(vec![
        Wave {
          id: "kickoff",
          title: "Snapfire kickoff",
          participants: vec!["alice".to_owned(), "bob".to_owned()],
          blips: vec![
            blip(1, "", "alice", "Starting a wave for the launch. Reply under a blip and it nests.", "09:10"),
            blip(2, "1", "bob", "Good. I will take the runtime half.", "09:12"),
            blip(3, "2", "alice", "Then I have the client. Watch this line while I type in the other window.", "09:13"),
            blip(4, "", "alice", "Anything that is its own subject goes at the top level.", "09:14"),
          ],
        },
        Wave {
          id: "board",
          title: "Arrivals board review",
          participants: vec!["alice".to_owned()],
          blips: vec![blip(5, "", "alice", "The panels stream. The clock is the part I want a second opinion on.", "08:02")],
        },
      ]),
      next: Mutex::new(6),
      topics: broadcast::channel(64).0,
    }
  }

  /// What the host forwards to `publish`: one topic per wave that gained a blip.
  pub fn changes(&self) -> broadcast::Receiver<String> {
    self.topics.subscribe()
  }

  fn summary(wave: &Wave) -> Value {
    let mut map = ValueMap::new();
    map.insert("id".to_owned(), Value::str(wave.id));
    map.insert("title".to_owned(), Value::str(wave.title));
    map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::Str(who.clone())).collect()));
    map.insert("blips".to_owned(), Value::F64(wave.blips.len() as f64));
    map.insert("last".to_owned(), Value::Str(wave.blips.last().map(|b| b.at.clone()).unwrap_or_default()));
    Value::Map(map)
  }

  /// Blips in reading order: every blip followed by its replies, each one
  /// carrying how deep it sits so the page needs no tree walk of its own.
  fn threaded(blips: &[Blip]) -> Vec<Value> {
    fn walk(blips: &[Blip], parent: &str, depth: f64, out: &mut Vec<Value>) {
      for blip in blips.iter().filter(|blip| blip.parent == parent) {
        let mut map = ValueMap::new();
        map.insert("id".to_owned(), Value::Str(blip.id.to_string()));
        map.insert("parent".to_owned(), Value::Str(blip.parent.clone()));
        map.insert("who".to_owned(), Value::Str(blip.who.clone()));
        map.insert("body".to_owned(), Value::Str(blip.body.clone()));
        map.insert("at".to_owned(), Value::Str(blip.at.clone()));
        map.insert("depth".to_owned(), Value::F64(depth));
        out.push(Value::Map(map));
        walk(blips, &blip.id.to_string(), depth + 1.0, out);
      }
    }
    let mut out = Vec::new();
    walk(blips, "", 0.0, &mut out);
    out
  }

  fn wave(&self, id: &str) -> Option<Value> {
    let waves = self.waves.lock();
    let wave = waves.iter().find(|wave| wave.id == id)?;
    let mut map = ValueMap::new();
    map.insert("id".to_owned(), Value::str(wave.id));
    map.insert("title".to_owned(), Value::str(wave.title));
    map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::Str(who.clone())).collect()));
    map.insert("blips".to_owned(), Value::Seq(Self::threaded(&wave.blips)));
    Some(Value::Map(map))
  }

  /// Keeps a blip, adds whoever wrote it to the participants and names the
  /// wave as a topic, which is how every other page learns to reload it.
  fn add(&self, id: &str, parent: &str, who: &str, body: &str) -> Option<Value> {
    let kept = {
      let mut waves = self.waves.lock();
      let wave = waves.iter_mut().find(|wave| wave.id == id)?;
      let mut next = self.next.lock();
      let kept = blip(*next, parent, who, body, &now());
      *next += 1;
      if !who.is_empty() && !wave.participants.iter().any(|there| there == who) {
        wave.participants.push(who.to_owned());
      }
      wave.blips.push(kept.clone());
      kept
    };
    let _ = self.topics.send(format!("wave/{id}"));
    let mut map = ValueMap::new();
    map.insert("id".to_owned(), Value::Str(kept.id.to_string()));
    map.insert("parent".to_owned(), Value::Str(kept.parent));
    map.insert("who".to_owned(), Value::Str(kept.who));
    map.insert("body".to_owned(), Value::Str(kept.body));
    map.insert("at".to_owned(), Value::Str(kept.at));
    map.insert("depth".to_owned(), Value::F64(0.0));
    Some(Value::Map(map))
  }
}

fn now() -> String {
  let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
  format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}

fn string(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(text)) => text.clone(),
    _ => String::new(),
  }
}

/// The waves service in process, and the handle the host publishes from.
pub fn waves() -> (Arc<dyn Transport>, Arc<Waves>) {
  let state = Arc::new(Waves::new());
  let (listing, reading, writing) = (state.clone(), state.clone(), state.clone());
  let transport = LocalTransport::new()
    .method("waves.listWaves", move |_| {
      let waves = Value::Seq(listing.waves.lock().iter().map(Waves::summary).collect());
      async move { Ok(waves) }
    })
    .method("waves.getWave", move |call| {
      let id = string(&call.args, "id");
      let wave = reading.wave(&id);
      async move { wave.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "getWave", format!("no wave `{id}`"))) }
    })
    .method("waves.addBlip", move |call| {
      let (id, parent, who, body) = (string(&call.args, "id"), string(&call.args, "parent"), string(&call.args, "who"), string(&call.args, "body"));
      let kept = writing.add(&id, &parent, &who, &body);
      async move { kept.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "addBlip", format!("no wave `{id}`"))) }
    });
  (Arc::new(transport), state)
}
