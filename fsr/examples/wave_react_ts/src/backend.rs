use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use plaza::{query_with, CommandSender, ControllerCommand};
use pulldown_cmark::{CowStr, Event, HeadingLevel, Parser, Tag};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{LocalTransport, Transport};

use crate::field::{Blip, Conn, Field, Op, Wave, Game};

pub type Waves = CommandSender<Op, Conn, Field>;

/// The waves the field starts with. A blip's `at` is the wall clock, since a
/// transcript is the one place a reader wants the real one.
pub fn seed() -> Vec<Wave> {
  let blip = |id: u64, parent: &str, anchor: &str, who: &str, body: &str, at: &str| Blip {
    id,
    parent: parent.to_owned(),
    anchor: anchor.to_owned(),
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
        blip(1, "", "", "alice", "Starting a wave for the launch. Reply under a blip and it nests.", "09:10"),
        blip(2, "1", "", "bob", "Good. I will take the runtime half.", "09:12"),
        blip(3, "2", "", "alice", "Then I have the client. Watch this line while I type in the other window.", "09:13"),
        blip(4, "", "", "alice", "Anything that is **its own subject** goes at the top level. A blip is [markdown](https://commonmark.org).\n\nA reply can answer one part of a blip:\n\n- the runtime\n- the client", "09:14"),
        blip(6, "4", "2.1", "bob", "The client is mine too.", "09:16"),
      ],
      game: Game::default(),
    },
    Wave {
      id: "board".to_owned(),
      title: "Arrivals board review".to_owned(),
      participants: vec!["alice".to_owned()],
      blips: vec![blip(5, "", "", "alice", "The panels stream. The clock is the part I want a second opinion on.", "08:02")],
      game: Game::default(),
    },
  ]
}

fn summary(wave: &Wave) -> Value {
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::str(wave.id.clone()));
  map.insert("title".to_owned(), Value::str(wave.title.clone()));
  map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::str(who.clone())).collect()));
  map.insert("blips".to_owned(), Value::F64(wave.blips.len() as f64));
  map.insert("last".to_owned(), Value::str(wave.blips.last().map(|blip| blip.at.clone()).unwrap_or_default()));
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
        map.insert("name".to_owned(), Value::str(name));
        Value::Map(map)
      })
      .collect(),
  )
}

/// Which waves a view names. `active` is whoever is connected now, `mine` is
/// every wave the reader has written in and anything else is all of them.
fn under(field: &Field, view: &str, who: &str) -> Value {
  let listed = field.waves.values().filter(|wave| match view {
    "active" => field.here.get(&wave.id).is_some_and(|here| !here.is_empty()),
    "mine" => !who.is_empty() && wave.blips.iter().any(|blip| blip.who == who),
    _ => true,
  });
  Value::Seq(listed.map(summary).collect())
}

/// One part of a blip's body: a node of its markdown, which the page renders
/// with a component that calls itself, so no string of markup is ever
/// written. Raw HTML in the source is a text part and a link or image keeps
/// its target only when that is http, https, mailto or has no scheme.
struct Part {
  kind: &'static str,
  /// A text part's text, a code block's or code span's source, an image's alt.
  text: String,
  href: String,
  children: Vec<Part>,
}

impl Part {
  fn new(kind: &'static str) -> Self {
    Self { kind, text: String::new(), href: String::new(), children: Vec::new() }
  }

  /// A block a reply can answer: a paragraph, a heading, a code block or a
  /// list item holding no block of its own, whose blocks are answered instead.
  fn answerable(&self) -> bool {
    const BLOCKS: [&str; 11] = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "pre", "ul", "ol", "blockquote"];
    match self.kind {
      "li" => !self.children.iter().any(|child| BLOCKS.contains(&child.kind)),
      kind => BLOCKS[..8].contains(&kind),
    }
  }
}

/// `body` parsed into parts. CommonMark only: tables, footnotes and the
/// other extensions are off, so their syntax stays text.
fn parts(body: &str) -> Vec<Part> {
  let mut open = vec![Part::new("root")];
  for event in Parser::new(body) {
    match event {
      Event::Start(tag) => open.push(opened(tag)),
      Event::End(_) if open.len() > 1 => {
        let done = open.pop().expect("more than the root is open");
        open.last_mut().expect("the root stays open").children.push(done);
      }
      Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => leaf(&mut open, &text),
      Event::SoftBreak => leaf(&mut open, "\n"),
      Event::Code(text) => push(&mut open, Part { text: text.to_string(), ..Part::new("code") }),
      Event::HardBreak => push(&mut open, Part::new("br")),
      Event::Rule => push(&mut open, Part::new("hr")),
      _ => {}
    }
  }
  open.swap_remove(0).children
}

fn opened(tag: Tag<'_>) -> Part {
  match tag {
    Tag::Paragraph | Tag::HtmlBlock => Part::new("p"),
    Tag::Heading { level, .. } => Part::new(match level {
      HeadingLevel::H1 => "h1",
      HeadingLevel::H2 => "h2",
      HeadingLevel::H3 => "h3",
      HeadingLevel::H4 => "h4",
      HeadingLevel::H5 => "h5",
      HeadingLevel::H6 => "h6",
    }),
    Tag::BlockQuote(_) => Part::new("blockquote"),
    Tag::CodeBlock(_) => Part::new("pre"),
    Tag::List(Some(_)) => Part::new("ol"),
    Tag::List(None) => Part::new("ul"),
    Tag::Item => Part::new("li"),
    Tag::Emphasis => Part::new("em"),
    Tag::Strong => Part::new("strong"),
    Tag::Link { dest_url, .. } => Part { href: safe_url(dest_url).to_string(), ..Part::new("a") },
    Tag::Image { dest_url, .. } => Part { href: safe_url(dest_url).to_string(), ..Part::new("img") },
    _ => Part::new("span"),
  }
}

fn push(open: &mut [Part], part: Part) {
  if let Some(top) = open.last_mut() {
    top.children.push(part);
  }
}

/// Text lands in the open part: as the source of a code block or an image's
/// alt, else as a text part of its own.
fn leaf(open: &mut [Part], text: &str) {
  let Some(top) = open.last_mut() else { return };
  match top.kind {
    "pre" | "img" => top.text.push_str(text),
    _ => top.children.push(Part { text: text.to_owned(), ..Part::new("text") }),
  }
}

fn safe_url(url: CowStr<'_>) -> CowStr<'_> {
  let scheme = url.split_once(':').map(|(scheme, _)| scheme).filter(|scheme| !scheme.contains(['/', '?', '#'])).map(str::to_ascii_lowercase);
  match scheme.as_deref() {
    None | Some("http" | "https" | "mailto") => url,
    Some(_) => CowStr::Borrowed("#"),
  }
}

/// `parts` as the page reads them. Every part has a path, its index under
/// each part above it joined by dots; a block a reply can answer carries its
/// path as `at` and the replies anchored there, taken out of `anchored`.
fn parts_value(parts: Vec<Part>, path: &str, anchored: &mut BTreeMap<String, Vec<Value>>) -> Value {
  let mut out = Vec::with_capacity(parts.len());
  for (i, part) in parts.into_iter().enumerate() {
    let at = if path.is_empty() { i.to_string() } else { format!("{path}.{i}") };
    let answerable = part.answerable();
    let mut map = ValueMap::default();
    map.insert("kind".to_owned(), Value::str(part.kind));
    map.insert("text".to_owned(), Value::str(part.text));
    map.insert("href".to_owned(), Value::str(part.href));
    map.insert("children".to_owned(), parts_value(part.children, &at, anchored));
    map.insert("replies".to_owned(), Value::seq(if answerable { anchored.remove(&at).unwrap_or_default() } else { Vec::new() }));
    map.insert("at".to_owned(), Value::str(if answerable { at } else { String::new() }));
    out.push(Value::Map(map));
  }
  Value::seq(out)
}

/// A blip as the page reads it: its body as parts, each reply anchored to a
/// block of it under that block and every other reply after it. A reply
/// whose block an amend took away joins the others rather than being lost.
fn blip_value(blip: &Blip, blips: &[Blip]) -> Value {
  let id = blip.id.to_string();
  let mut replies = Vec::new();
  let mut anchored: BTreeMap<String, Vec<Value>> = BTreeMap::new();
  for reply in blips.iter().filter(|reply| reply.parent == id) {
    let value = blip_value(reply, blips);
    match reply.anchor.is_empty() {
      true => replies.push(value),
      false => anchored.entry(reply.anchor.clone()).or_default().push(value),
    }
  }
  let parts = parts_value(parts(&blip.body), "", &mut anchored);
  replies.extend(anchored.into_values().flatten());
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::str(id));
  map.insert("parent".to_owned(), Value::str(blip.parent.clone()));
  map.insert("anchor".to_owned(), Value::str(blip.anchor.clone()));
  map.insert("who".to_owned(), Value::str(blip.who.clone()));
  map.insert("body".to_owned(), Value::str(blip.body.clone()));
  map.insert("at".to_owned(), Value::str(blip.at.clone()));
  map.insert("edited".to_owned(), Value::str(blip.edited.clone()));
  map.insert("editors".to_owned(), Value::Seq(blip.editors.iter().map(|who| Value::str(who.clone())).collect()));
  map.insert("parts".to_owned(), parts);
  map.insert("replies".to_owned(), Value::seq(replies));
  Value::Map(map)
}

fn wave_value(field: &Field, id: &str) -> Option<Value> {
  let wave = field.waves.get(id)?;
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::str(wave.id.clone()));
  map.insert("title".to_owned(), Value::str(wave.title.clone()));
  map.insert("participants".to_owned(), Value::Seq(wave.participants.iter().map(|who| Value::str(who.clone())).collect()));
  map.insert("blips".to_owned(), Value::seq(wave.blips.iter().filter(|blip| blip.parent.is_empty()).map(|blip| blip_value(blip, &wave.blips)).collect::<Vec<_>>()));
  map.insert("game".to_owned(), game_value(&wave.game));
  Some(Value::Map(map))
}

/// The gadget as the page reads it: the nine cells as their own rows, so the
/// markup is a loop rather than nine copies.
fn game_value(game: &Game) -> Value {
  let cells = game
    .cells
    .chars()
    .enumerate()
    .map(|(i, mark)| {
      let mut cell = ValueMap::default();
      cell.insert("at".to_owned(), Value::Int(i as i128));
      cell.insert("mark".to_owned(), Value::str(if mark == '.' { String::new() } else { mark.to_string() }));
      Value::Map(cell)
    })
    .collect::<Vec<_>>();
  let mut map = ValueMap::default();
  map.insert("cells".to_owned(), Value::seq(cells));
  map.insert("turn".to_owned(), Value::str(game.turn.clone()));
  map.insert("won".to_owned(), Value::str(game.won.clone()));
  Some(Value::Map(map)).unwrap()
}

fn string(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(text)) => text.to_string(),
    _ => String::new(),
  }
}

fn gone(method: &'static str) -> ServiceError {
  ServiceError::new(FailureKind::Unavailable, "waves", method, "the field is not running")
}

/// The service the application's loaders and actions call, over the one
/// controller that owns the state. A read is a closure the controller runs on
/// its own task; a write is an op and the query after it returns only once
/// that op has been applied, since the controller does one thing at a time.
pub fn service(field: Waves) -> (Arc<dyn Transport>, fibre::mpsc::UnboundedAsyncReceiver<String>) {
  let (told, hear) = fibre::mpsc::unbounded();
  let (amended, kept, played) = (told.clone(), told.clone(), told);
  let (listing, counting, reading, writing, amending, playing) =
    (field.clone(), field.clone(), field.clone(), field.clone(), field.clone(), field);
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
            field.waves.get(&id).and_then(|wave| wave.blips.iter().find(|held| held.id.to_string() == blip).map(|held| blip_value(held, &wave.blips)))
          })
          .await
          .map_err(|_| gone("editBlip"))?;
          let _ = told.send(topic);
          amended.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "editBlip", "no such blip"))
        }
      })
      .method("waves.play", move |call| {
        let field = playing.clone();
        let mut told = played.clone();
        let (id, who) = (string(&call.args, "id"), string(&call.args, "who"));
        let cell = match call.args.get("cell") {
          Some(Value::Int(at)) if *at >= 0 => Some(*at as usize),
          Some(Value::F64(at)) if *at >= 0.0 => Some(*at as usize),
          _ => None,
        };
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Play { wave: id.clone(), who, cell };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("play"))?;
          let played = query_with(&field, move |field| field.waves.get(&id).map(|wave| game_value(&wave.game))).await.map_err(|_| gone("play"))?;
          let _ = told.send(topic);
          played.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "play", "no such wave"))
        }
      })
      .method("waves.addBlip", move |call| {
        let field = writing.clone();
        let mut told = kept.clone();
        let (id, parent, anchor) = (string(&call.args, "id"), string(&call.args, "parent"), string(&call.args, "anchor"));
        let (who, body) = (string(&call.args, "who"), string(&call.args, "body"));
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Keep { wave: id.clone(), parent, anchor, who, body };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("addBlip"))?;
          let kept = query_with(&field, move |field| {
            field.waves.get(&id).and_then(|wave| wave.blips.last().map(|blip| blip_value(blip, &wave.blips)))
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
