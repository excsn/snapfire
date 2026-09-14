use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use plaza::{query_with, CommandSender, ControllerCommand};
use pulldown_cmark::{BrokenLink, CowStr, Event, HeadingLevel, Options, Parser, RefDefs, Tag};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{LocalTransport, Transport};

use crate::blocks::Block;
use crate::field::{Blip, Change, Conn, Field, Op, Wave};
use crate::gadgets::{self, Game, Kind, State};

pub type Waves = CommandSender<Op, Conn, Field>;

/// The waves the field starts with, each written as its log so playback
/// starts from an empty wave. A blip's `at` is the wall clock, since a
/// transcript is the one place a reader wants the real one.
pub fn seed() -> Vec<Wave> {
  let wave = |id: &str, title: &str, blips: &[(u64, &str, &str, &str, &str, &str)]| {
    let mut wave = Wave::new(id, title);
    for (id, parent, anchor, who, body, at) in blips {
      let (parent, anchor, who, body, at) = ((*parent).to_owned(), (*anchor).to_owned(), (*who).to_owned(), (*body).to_owned(), (*at).to_owned());
      wave.apply(Change::Kept { id: *id, parent, anchor, who, body, at });
    }
    wave
  };
  vec![
    wave(
      "kickoff",
      "Snapfire kickoff",
      &[
        (1, "", "", "alice", "Starting a wave for the launch. Reply under a blip and it nests.", "09:10"),
        (2, "1", "", "bob", "Good. I will take the runtime half.", "09:12"),
        (3, "2", "", "alice", "Then I have the client. Watch this line while I type in the other window.", "09:13"),
        (4, "", "", "alice", "Anything that is **its own subject** goes at the top level. A blip is [markdown](https://commonmark.org).\n\nA reply can answer one part of a blip:\n\n- the runtime\n- the client", "09:14"),
        (6, "4", "b3.1", "bob", "The client is mine too.", "09:16"),
        (7, "", "", "bob", "Something for the wait while the host builds.\n\n```gadget noughts\n```", "09:18"),
        (8, "", "", "alice", "```gadget poll\nWhen do we launch?\nThursday\nFriday\nNext week\n```", "09:20"),
      ],
    ),
    wave(
      "board",
      "Arrivals board review",
      &[
        (5, "", "", "alice", "The panels stream. The clock is the part I want a second opinion on.", "08:02"),
        (9, "5", "", "alice", "```gadget yesno\nKeep the clock in the corner?\n```", "08:03"),
      ],
    ),
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

  /// A block a reply can answer: a paragraph, a heading, a code block, a
  /// gadget or a list item holding no block of its own, whose blocks are
  /// answered instead.
  fn answerable(&self) -> bool {
    const BLOCKS: [&str; 11] = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "pre", "ul", "ol", "blockquote"];
    match self.kind {
      "li" => !self.children.iter().any(|child| BLOCKS.contains(&child.kind)),
      "gadget" => true,
      kind => BLOCKS[..8].contains(&kind),
    }
  }
}

/// One block's `text` parsed into parts, a reference link in it resolved
/// against `defs`, the definitions anywhere in the blip. CommonMark only:
/// tables, footnotes and the other extensions are off, so their syntax stays
/// text.
fn parts(text: &str, defs: &RefDefs<'_>) -> Vec<Part> {
  let resolve = |link: BrokenLink<'_>| {
    defs.get(link.reference.as_ref()).map(|def| (CowStr::from(def.dest.to_string()), CowStr::from(def.title.as_deref().unwrap_or_default().to_owned())))
  };
  let mut open = vec![Part::new("root")];
  for event in Parser::new_with_broken_link_callback(text, Options::empty(), Some(resolve)) {
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

/// One part as the page reads it, at path `at`. A part a reply can answer
/// carries its path as `at` and the replies anchored there, taken out of
/// `anchored`; its children's paths extend it.
fn part_value(part: Part, at: String, anchored: &mut BTreeMap<String, Vec<Value>>) -> Value {
  let answerable = part.answerable();
  let mut map = ValueMap::default();
  map.insert("kind".to_owned(), Value::str(part.kind));
  map.insert("text".to_owned(), Value::str(part.text));
  map.insert("href".to_owned(), Value::str(part.href));
  map.insert("children".to_owned(), parts_value(part.children, &at, anchored));
  map.insert("replies".to_owned(), Value::seq(if answerable { anchored.remove(&at).unwrap_or_default() } else { Vec::new() }));
  map.insert("at".to_owned(), Value::str(if answerable { at } else { String::new() }));
  Value::Map(map)
}

/// `parts` under the part at `path`, each at its index joined on with a dot.
fn parts_value(parts: Vec<Part>, path: &str, anchored: &mut BTreeMap<String, Vec<Value>>) -> Value {
  let mut out = Vec::with_capacity(parts.len());
  for (i, part) in parts.into_iter().enumerate() {
    out.push(part_value(part, format!("{path}.{i}"), anchored));
  }
  Value::seq(out)
}

/// A blip's blocks as the page reads them: each with its id, its source, its
/// parts and its gadget. A block's first part is at the block's id, so a reply
/// anchored to it follows the block wherever an edit moves it; a list item in
/// it is at the id and its path below. A gadget block is one part of kind
/// `gadget`, which a reply can answer too.
fn blocks_value(blip: &Blip, lit: Lit<'_>, anchored: &mut BTreeMap<String, Vec<Value>>) -> Value {
  let id = blip.id.to_string();
  let whole = blip.body();
  let mut reading = Parser::new(&whole);
  reading.by_ref().for_each(drop);
  let defs = reading.reference_definitions();
  let mut out = Vec::with_capacity(blip.blocks.len());
  for block in &blip.blocks {
    let kind = gadgets::kind_of(&block.text);
    let mut parts = Vec::new();
    match kind {
      Some(_) => parts.push(part_value(Part::new("gadget"), block.id.clone(), anchored)),
      None => {
        for (i, part) in self::parts(&block.text, defs).into_iter().enumerate() {
          parts.push(part_value(part, if i == 0 { block.id.clone() } else { format!("{}+{i}", block.id) }, anchored));
        }
      }
    }
    let mut map = ValueMap::default();
    map.insert("id".to_owned(), Value::str(block.id.clone()));
    map.insert("text".to_owned(), Value::str(block.text.clone()));
    map.insert("parts".to_owned(), Value::seq(parts));
    map.insert("gadget".to_owned(), gadget_value(blip, block, kind.as_ref(), lit.blip == id && lit.block == block.id));
    out.push(Value::Map(map));
  }
  Value::seq(out)
}

/// What a step of playback changed: a blip or one gadget block of it when
/// `block` is not empty. Nothing on the wave as it stands.
#[derive(Clone, Copy, Default)]
struct Lit<'a> {
  blip: &'a str,
  block: &'a str,
}

/// A blip as the page reads it: its body as blocks of parts, each reply
/// anchored to a block under that block and every other reply after it. A
/// reply whose block an amend took away joins the others rather than being
/// lost. It is lit when `lit` names it and no block of it.
fn blip_value(blip: &Blip, blips: &[Blip], lit: Lit<'_>) -> Value {
  let id = blip.id.to_string();
  let mut replies = Vec::new();
  let mut anchored: BTreeMap<String, Vec<Value>> = BTreeMap::new();
  for reply in blips.iter().filter(|reply| reply.parent == id) {
    let value = blip_value(reply, blips, lit);
    match reply.anchor.is_empty() {
      true => replies.push(value),
      false => anchored.entry(reply.anchor.clone()).or_default().push(value),
    }
  }
  let blocks = blocks_value(blip, lit, &mut anchored);
  replies.extend(anchored.into_values().flatten());
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::str(id));
  map.insert("parent".to_owned(), Value::str(blip.parent.clone()));
  map.insert("anchor".to_owned(), Value::str(blip.anchor.clone()));
  map.insert("who".to_owned(), Value::str(blip.who.clone()));
  map.insert("body".to_owned(), Value::str(blip.body()));
  map.insert("at".to_owned(), Value::str(blip.at.clone()));
  map.insert("edited".to_owned(), Value::str(blip.edited.clone()));
  map.insert("editors".to_owned(), Value::Seq(blip.editors.iter().map(|who| Value::str(who.clone())).collect()));
  map.insert("blocks".to_owned(), blocks);
  map.insert("replies".to_owned(), Value::seq(replies));
  map.insert("lit".to_owned(), Value::Bool(lit.block.is_empty() && lit.blip == blip.id.to_string()));
  Value::Map(map)
}

/// The wave as the page reads it: replayed to step `at` of its log or as it
/// stands when `at` is empty or not before the end. A step of playback lights
/// the blip or the board its change touched.
fn wave_value(field: &Field, id: &str, at: &str) -> Option<Value> {
  let wave = field.waves.get(id)?;
  let steps = wave.log.len();
  let step = at.parse::<usize>().ok().filter(|at| *at < steps).unwrap_or(steps);
  let live = step == steps;
  let replayed;
  let shown = match live {
    true => wave,
    false => {
      replayed = wave.replayed(step);
      &replayed
    }
  };
  let change = step.checked_sub(1).and_then(|last| wave.log.get(last));
  let (blip, block) = match (live, change) {
    (false, Some(Change::Kept { id, .. })) => (id.to_string(), ""),
    (false, Some(Change::Amended { blip, .. })) => (blip.clone(), ""),
    (false, Some(Change::Played { blip, block, .. } | Change::Cleared { blip, block, .. } | Change::Voted { blip, block, .. })) => (blip.clone(), block.as_str()),
    _ => (String::new(), ""),
  };
  let lit = Lit { blip: &blip, block };
  let mut map = ValueMap::default();
  map.insert("id".to_owned(), Value::str(wave.id.clone()));
  map.insert("title".to_owned(), Value::str(wave.title.clone()));
  map.insert("participants".to_owned(), Value::Seq(shown.participants.iter().map(|who| Value::str(who.clone())).collect()));
  map.insert("blips".to_owned(), Value::seq(shown.blips.iter().filter(|blip| blip.parent.is_empty()).map(|blip| blip_value(blip, &shown.blips, lit)).collect::<Vec<_>>()));
  map.insert("step".to_owned(), Value::F64(step as f64));
  map.insert("steps".to_owned(), Value::F64(steps as f64));
  map.insert("live".to_owned(), Value::Bool(live));
  map.insert("change".to_owned(), change_value(change));
  Some(Value::Map(map))
}

/// What one change in the log did, for the scrubber to say: its kind (`kept`,
/// `amended`, `played`, `cleared` or `voted`), who made it, when and the blip
/// it touched. Every field is empty before the first change.
fn change_value(change: Option<&Change>) -> Value {
  let (kind, who, at, blip) = match change {
    Some(Change::Kept { id, who, at, .. }) => ("kept", who.as_str(), at.as_str(), id.to_string()),
    Some(Change::Amended { blip, who, at, .. }) => ("amended", who.as_str(), at.as_str(), blip.clone()),
    Some(Change::Played { blip, who, at, .. }) => ("played", who.as_str(), at.as_str(), blip.clone()),
    Some(Change::Cleared { blip, who, at, .. }) => ("cleared", who.as_str(), at.as_str(), blip.clone()),
    Some(Change::Voted { blip, who, at, .. }) => ("voted", who.as_str(), at.as_str(), blip.clone()),
    None => ("", "", "", String::new()),
  };
  let mut map = ValueMap::default();
  map.insert("kind".to_owned(), Value::str(kind));
  map.insert("who".to_owned(), Value::str(who));
  map.insert("at".to_owned(), Value::str(at));
  map.insert("blip".to_owned(), Value::str(blip));
  Value::Map(map)
}

/// A block's gadget as the page reads it, every field empty when the block is
/// no gadget: its kind, the question a vote asks and a board's cells with
/// whose turn it is and who has won. A vote carries each answer it offers
/// with everyone who gave it. `lit` when a step of playback moved on it.
fn gadget_value(blip: &Blip, block: &Block, kind: Option<&Kind>, lit: bool) -> Value {
  let state = kind.and_then(|kind| blip.gadgets.get(&block.id).filter(|state| kind.fits(state)));
  let board = matches!(kind, Some(Kind::Noughts));
  let game = match state {
    Some(State::Board(game)) => game.clone(),
    _ => Game::default(),
  };
  let cells = match board {
    true => game
      .cells
      .chars()
      .enumerate()
      .map(|(i, mark)| {
        let mut cell = ValueMap::default();
        cell.insert("at".to_owned(), Value::Int(i as i128));
        cell.insert("mark".to_owned(), Value::str(if mark == '.' { String::new() } else { mark.to_string() }));
        Value::Map(cell)
      })
      .collect(),
    false => Vec::new(),
  };
  let votes = match state {
    Some(State::Votes(votes)) => votes.clone(),
    _ => BTreeMap::new(),
  };
  let choices = kind
    .map(Kind::answers)
    .unwrap_or_default()
    .into_iter()
    .map(|answer| {
      let who: Vec<Value> = votes.iter().filter(|(_, given)| **given == answer).map(|(who, _)| Value::str(who.clone())).collect();
      let mut choice = ValueMap::default();
      choice.insert("count".to_owned(), Value::F64(who.len() as f64));
      choice.insert("who".to_owned(), Value::seq(who));
      choice.insert("answer".to_owned(), Value::str(answer));
      Value::Map(choice)
    })
    .collect::<Vec<_>>();
  let mut map = ValueMap::default();
  map.insert("kind".to_owned(), Value::str(kind.map(Kind::name).unwrap_or_default()));
  map.insert("question".to_owned(), Value::str(kind.map(Kind::question).unwrap_or_default().to_owned()));
  map.insert("cells".to_owned(), Value::seq(cells));
  map.insert("turn".to_owned(), Value::str(if board { game.turn.clone() } else { String::new() }));
  map.insert("won".to_owned(), Value::str(if board { game.won.clone() } else { String::new() }));
  map.insert("choices".to_owned(), Value::seq(choices));
  map.insert("lit".to_owned(), Value::Bool(lit));
  Value::Map(map)
}

/// Blip `blip` of wave `wave` as the page reads it, its replies under it.
fn held_blip(field: &Field, wave: &str, blip: &str) -> Option<Value> {
  let wave = field.waves.get(wave)?;
  wave.blips.iter().find(|held| held.id.to_string() == blip).map(|held| blip_value(held, &wave.blips, Lit::default()))
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
  let (amended, kept, played, voted) = (told.clone(), told.clone(), told.clone(), told);
  let (listing, counting, reading, writing, amending, playing, voting) =
    (field.clone(), field.clone(), field.clone(), field.clone(), field.clone(), field.clone(), field);
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
        let (id, at) = (string(&call.args, "id"), string(&call.args, "at"));
        async move {
          let wave = query_with(&field, move |field| wave_value(field, &id, &at)).await.map_err(|_| gone("getWave"))?;
          wave.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "getWave", "no such wave"))
        }
      })
      .method("waves.editBlip", move |call| {
        let field = amending.clone();
        let mut told = amended.clone();
        let (id, blip, block) = (string(&call.args, "id"), string(&call.args, "blip"), string(&call.args, "block"));
        let (who, body) = (string(&call.args, "who"), string(&call.args, "body"));
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Amend { wave: id.clone(), blip: blip.clone(), block, who, body };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("editBlip"))?;
          let amended = query_with(&field, move |field| held_blip(field, &id, &blip)).await.map_err(|_| gone("editBlip"))?;
          let _ = told.send(topic);
          amended.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "editBlip", "no such blip"))
        }
      })
      .method("waves.play", move |call| {
        let field = playing.clone();
        let mut told = played.clone();
        let (id, blip, block, who) = (string(&call.args, "id"), string(&call.args, "blip"), string(&call.args, "block"), string(&call.args, "who"));
        let cell = match call.args.get("cell") {
          Some(Value::Int(at)) if *at >= 0 => Some(*at as usize),
          Some(Value::F64(at)) if *at >= 0.0 => Some(*at as usize),
          _ => None,
        };
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Play { wave: id.clone(), blip: blip.clone(), block, who, cell };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("play"))?;
          let played = query_with(&field, move |field| held_blip(field, &id, &blip)).await.map_err(|_| gone("play"))?;
          let _ = told.send(topic);
          played.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "play", "no such blip"))
        }
      })
      .method("waves.vote", move |call| {
        let field = voting.clone();
        let mut told = voted.clone();
        let (id, blip, block) = (string(&call.args, "id"), string(&call.args, "blip"), string(&call.args, "block"));
        let (who, answer) = (string(&call.args, "who"), string(&call.args, "answer"));
        async move {
          let topic = format!("wave/{id}");
          let op = Op::Vote { wave: id.clone(), blip: blip.clone(), block, who, answer };
          field
            .send(ControllerCommand::SubmitSystemOps { source_description: "an action".to_owned(), ops: vec![op] })
            .await
            .map_err(|_| gone("vote"))?;
          let answered = query_with(&field, move |field| held_blip(field, &id, &blip)).await.map_err(|_| gone("vote"))?;
          let _ = told.send(topic);
          answered.ok_or_else(|| ServiceError::new(FailureKind::NotFound, "waves", "vote", "no such blip"))
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
            field.waves.get(&id).and_then(|wave| wave.blips.last().map(|blip| blip_value(blip, &wave.blips, Lit::default())))
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
