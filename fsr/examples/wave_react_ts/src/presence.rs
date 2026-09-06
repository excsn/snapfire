use std::collections::BTreeMap;

use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::socket::{On, Reply, Row, Who};

/// Who is on each wave and what they are part way through typing. The
/// framework holds the sockets; what a connection means is this. Keyed by
/// connection rather than by name, so two windows of one person are two
/// presences and closing one does not take the other's name off the wave.
#[derive(Default)]
pub struct Field {
  here: Mutex<BTreeMap<String, BTreeMap<u64, String>>>,
  drafts: Mutex<BTreeMap<String, BTreeMap<u64, (String, String, String)>>>,
}

impl Field {
  pub fn new() -> Self {
    Self::default()
  }

  /// The seam the host calls: a join, a row or a leave, answered with the
  /// rows every open page on that wave should take.
  pub fn on(&self, who: &Who, event: On) -> Reply {
    let name = match who.session.get("name") {
      Some(Value::Str(name)) if !name.is_empty() => name,
      _ => "someone".to_owned(),
    };
    match event {
      On::Joined => {
        self.here.lock().entry(who.topic.clone()).or_default().insert(who.connection, name);
        Reply::everyone([self.here_row(&who.topic)])
      }
      On::Left => {
        self.forget(&who.topic, who.connection);
        Reply::everyone([self.here_row(&who.topic), self.drafts_row(&who.topic)])
      }
      On::Said(row) if row.key == "typing" => {
        let (parent, body) = match &row.value {
          Value::Map(map) => (text(map, "parent"), text(map, "body")),
          _ => (String::new(), String::new()),
        };
        let mut drafts = self.drafts.lock();
        let wave = drafts.entry(who.topic.clone()).or_default();
        match body.is_empty() {
          true => {
            wave.remove(&who.connection);
          }
          false => {
            wave.insert(who.connection, (name, parent, body));
          }
        }
        drop(drafts);
        Reply::others([self.drafts_row(&who.topic)])
      }
      On::Said(_) => Reply::default(),
    }
  }

  fn forget(&self, topic: &str, connection: u64) {
    if let Some(wave) = self.here.lock().get_mut(topic) {
      wave.remove(&connection);
    }
    if let Some(wave) = self.drafts.lock().get_mut(topic) {
      wave.remove(&connection);
    }
  }

  /// `wave/here`: the names on the wave right now, each once however many
  /// windows they have open. The key names what the page is showing rather
  /// than which wave, since a document shows one, and a key the build can
  /// read is a key an island can be lowered around.
  fn here_row(&self, topic: &str) -> Row {
    let here = self.here.lock();
    let mut names: Vec<String> = here.get(topic).map(|wave| wave.values().cloned().collect()).unwrap_or_default();
    names.sort();
    names.dedup();
    Row::new("wave/here", Value::Seq(names.into_iter().map(Value::Str).collect()))
  }

  /// `wave/drafts`: what everyone is part way through typing, by the blip
  /// they are answering, which is what puts a draft in the right place.
  fn drafts_row(&self, topic: &str) -> Row {
    let drafts = self.drafts.lock();
    let rows: Vec<Value> = drafts
      .get(topic)
      .map(|wave| {
        wave
          .values()
          .map(|(who, parent, body)| {
            let mut map = ValueMap::new();
            map.insert("who".to_owned(), Value::Str(who.clone()));
            map.insert("parent".to_owned(), Value::Str(parent.clone()));
            map.insert("body".to_owned(), Value::Str(body.clone()));
            Value::Map(map)
          })
          .collect()
      })
      .unwrap_or_default();
    Row::new("wave/drafts", Value::Seq(rows))
  }
}

fn text(map: &ValueMap, key: &str) -> String {
  match map.get(key) {
    Some(Value::Str(text)) => text.clone(),
    _ => String::new(),
  }
}
