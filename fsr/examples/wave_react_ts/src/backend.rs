use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use plaza::{query_with, CommandSender, ControllerCommand};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{LocalTransport, Transport};

use crate::field::{Blip, Conn, Field, Op, Wave};

pub type Waves = CommandSender<Op, Conn, Field>;

/// The waves the field starts with. A blip's `at` is the wall clock, since a
/// transcript is the one place a reader wants the real one.
pub fn seed() -> Vec<Wave> {
  let blip = |id: u64, parent: &str, who: &str, body: &str, at: &str| Blip {
    id,
    parent: parent.to_owned(),
    who: who.to_owned(),
    body: body.to_owned(),
    at: at.to_owned(),
    edited: String::new(),
    editors: Vec::new(),
  };
  vec![
    Wave {
      id: "kickoff".to_owned(),
      title: "Snapfire kickoff".to_owned(),
      participants: vec!["alice".to_owned(), "bob".to_owned()],
      blips: vec![
        blip(1, "", "alice", "Starting a wave for the launch. Reply under a blip and it nests.", "09:10"),
        blip(2, "1", "bob", "Good. I will take the runtime half.", "09:12"),
        blip(3, "2", "alice", "Then I have the client. Watch this line while I type in the other window.", "09:13"),
        blip(4, "", "alice", "Anything that is its own subject goes at the top level.", "09:14"),
      ],
    },
    Wave {
      id: "board".to_owned(),
      title: "Arrivals board review".to_owned(),
      participants: vec!["alice".to_owned()],
      blips: vec![blip(5, "", "alice", "The panels stream. The clock is the part I want a second opinion on.", "08:02")],
    },
  ]
}

fn summary(wave: &Wave) -> Value {
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::Str(wave.id.clone()));
  map.insert("title".to_owned(), Value::Str(wave.title.clone()));
  map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::Str(who.clone())).collect()));
  map.insert("blips".to_owned(), Value::F64(wave.blips.len() as f64));
  map.insert("last".to_owned(), Value::Str(wave.blips.last().map(|blip| blip.at.clone()).unwrap_or_default()));
  Value::Map(map)
}

/// Everyone on any wave, with whoever is connected to any wave right now
/// marked. Presence is per connection, so a name is here when any window
/// holding it is.
fn people(field: &Field) -> Value {
  let mut counts: BTreeMap<String, f64> = BTreeMap::new();
  for wave in field.waves.values() {
    for who in &wave.participants {
      *counts.entry(who.clone()).or_default() += 1.0;
    }
  }
  let here: BTreeSet<&String> = field.here.values().flat_map(|here| here.values()).collect();
  Value::Seq(
    counts
      .into_iter()
      .map(|(name, waves)| {
        let mut map = ValueMap::default();
        map.insert("here".to_owned(), Value::Bool(here.contains(&name)));
        map.insert("waves".to_owned(), Value::F64(waves));
        map.insert("name".to_owned(), Value::Str(name));
        Value::Map(map)
      })
      .collect(),
  )
}

/// Which waves a view names. `active` is whoever is connected now, `mine` is
/// every wave the reader has written in, and anything else is all of them.
fn under(field: &Field, view: &str, who: &str) -> Value {
  let listed = field.waves.values().filter(|wave| match view {
    "active" => field.here.get(&wave.id).is_some_and(|here| !here.is_empty()),
    "mine" => !who.is_empty() && wave.blips.iter().any(|blip| blip.who == who),
    _ => true,
  });
  Value::Seq(listed.map(summary).collect())
}

fn blip_value(blip: &Blip, depth: f64) -> Value {
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::Str(blip.id.to_string()));
  map.insert("parent".to_owned(), Value::Str(blip.parent.clone()));
  map.insert("who".to_owned(), Value::Str(blip.who.clone()));
  map.insert("body".to_owned(), Value::Str(blip.body.clone()));
  map.insert("at".to_owned(), Value::Str(blip.at.clone()));
  map.insert("edited".to_owned(), Value::Str(blip.edited.clone()));
  map.insert("editors".to_owned(), Value::Seq(blip.editors.iter().map(|who| Value::Str(who.clone())).collect()));
  map.insert("depth".to_owned(), Value::F64(depth));
  Value::Map(map)
}

/// Blips in reading order: every blip followed by its replies, each carrying
/// how deep it sits, so the page needs no tree walk of its own.
fn threaded(blips: &[Blip]) -> Vec<Value> {
  fn walk(blips: &[Blip], parent: &str, depth: f64, out: &mut Vec<Value>) {
    for blip in blips.iter().filter(|blip| blip.parent == parent) {
      out.push(blip_value(blip, depth));
      walk(blips, &blip.id.to_string(), depth + 1.0, out);
    }
  }
  let mut out = Vec::new();
  walk(blips, "", 0.0, &mut out);
  out
}

fn wave_value(field: &Field, id: &str) -> Option<Value> {
  let wave = field.waves.get(id)?;
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::Str(wave.id.clone()));
  map.insert("title".to_owned(), Value::Str(wave.title.clone()));
  map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::Str(who.clone())).collect()));
  map.insert("blips".to_owned(), Value::Seq(threaded(&wave.blips)));
  Some(Value::Map(map))
}

fn string(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(text)) => text.clone(),
    _ => String::new(),
  }
}

fn gone(method: &'static str) -> ServiceError {
  ServiceError::new(FailureKind::Unavailable, "waves", method, "the field is not running")
}

/// The service the application's loaders and actions call, over the one
/// controller that owns the state. A read is a closure the controller runs on
/// its own task; a write is an op, and the query after it returns only once
/// that op has been applied, since the controller does one thing at a time.
pub fn service(field: Waves) -> (Arc<dyn Transport>, fibre::mpsc::UnboundedAsyncReceiver<String>) {
  let (told, hear) = fibre::mpsc::unbounded();
  let (amended, kept) = (told.clone(), told);
  let (listing, counting, reading, writing, amending) = (field.clone(), field.clone(), field.clone(), field.clone(), field);
  let transport: Arc<dyn Transport> = Arc::new(
    LocalTransport::new()
      .method("waves.listWaves", move |call| {
        let field = listing.clone();
        let (view, who) = (string(&call.args, "view"), string(&call.args, "who"));
        async move { query_with(&field, move |field| under(field, &view, &who)).await.map_err(|_| gone("listWaves")) }
      })
      .method("waves.listPeople", move |_| {
        let field = counting.clone();
        async move { query_with(&field, people).await.map_err(|_| gone("listPeople")) }
      })
      .method("waves.getWave", move |call| {
        let field = reading.clone();
        let id = string(&call.args, "id");
        async move {
          let wave = query_with(&field, move |field| wave_value(field, &id)).await.map_err(|_| gone("getWave"))?;
          wave.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "getWave", "no such wave"))
        }
      })
      .method("waves.editBlip", move |call| {
        let field = amending.clone();
        let mut told = amended.clone();
        let (id, blip, who, body) = (string(&call.args, "id"), string(&call.args, "blip"), string(&call.args, "who"), string(&call.args, "body"));
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Amend { wave: id.clone(), blip: blip.clone(), who, body };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("editBlip"))?;
          let amended = query_with(&field, move |field| {
            field.waves.get(&id).and_then(|wave| wave.blips.iter().find(|held| held.id.to_string() == blip).map(|held| blip_value(held, 0.0)))
          })
          .await
          .map_err(|_| gone("editBlip"))?;
          let _ = told.send(topic);
          amended.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "editBlip", "no such blip"))
        }
      })
      .method("waves.addBlip", move |call| {
        let field = writing.clone();
        let mut told = kept.clone();
        let (id, parent, who, body) = (string(&call.args, "id"), string(&call.args, "parent"), string(&call.args, "who"), string(&call.args, "body"));
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Keep { wave: id.clone(), parent, who, body };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("addBlip"))?;
          let kept = query_with(&field, move |field| {
            field.waves.get(&id).and_then(|wave| wave.blips.last().map(|blip| blip_value(blip, 0.0)))
          })
          .await
          .map_err(|_| gone("addBlip"))?;
          let _ = told.send(topic);
          kept.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "addBlip", "no such wave"))
        }
      }),
  );
  (transport, hear.to_async())
}
