use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use plaza::error::{SnapshotError, StateLogicError};
use plaza::snapshot::SnapshotContext;
use plaza::state_logic::{LogicOutput, SnapshotRequest};
use plaza::{Agent, LogicInput, SnapshotProvider, StateLogic};
use serde::{Deserialize, Serialize};

use crate::blocks::{self, Block};
use crate::gadgets::{self, Game, Kind, State};

/// A connection, which is what plaza calls an agent here. Two windows of one
/// person are two agents, because presence and a draft belong to a window.
pub type Conn = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Blip {
  pub id: u64,
  pub parent: String,
  /// The block of the parent this answers: its id, then the path of a list item
  /// inside it when the reply answers one (`b3.1`); empty for a reply to the
  /// whole blip.
  pub anchor: String,
  pub who: String,
  /// The body, one top-level markdown block each. A block keeps its id while
  /// its text changes, so a reply anchored to it stays with it.
  pub blocks: Vec<Block>,
  /// How many block ids this blip has handed out.
  pub made: u64,
  pub at: String,
  /// When it was last amended, empty while it still says what it first said.
  pub edited: String,
  /// Everyone who has rewritten it, in the order they first did. The author
  /// is `who` and is not repeated here unless they came back to it.
  pub editors: Vec<String>,
  /// The state of each gadget block, by block id. A gadget nobody has used
  /// yet has none.
  pub gadgets: BTreeMap<String, State>,
}

impl Blip {
  /// A blip as first written: `body` split into blocks, each with an id of its own.
  pub fn written(id: u64, parent: &str, anchor: &str, who: &str, body: &str, at: String) -> Self {
    let mut blip = Self { id, parent: parent.to_owned(), anchor: anchor.to_owned(), who: who.to_owned(), blocks: Vec::new(), made: 0, at, edited: String::new(), editors: Vec::new(), gadgets: BTreeMap::new() };
    blip.blocks = blocks::assign(&[], blocks::split(body), &mut blip.made);
    blip
  }

  /// The markdown as one text, its blocks a blank line apart: what a rewrite
  /// of the whole blip starts from.
  pub fn body(&self) -> String {
    self.blocks.iter().map(|block| block.text.as_str()).collect::<Vec<_>>().join("\n\n")
  }

  /// `body` in place of block `block` or of the whole blip when `block` is
  /// empty. A block's text becomes as many blocks as `body` holds, the first
  /// keeping the block's id. An empty `body` removes the block. The whole
  /// blip's text becomes its blocks, each keeping its id where the new text
  /// kept or edited it. False when the blip has no such block.
  fn rewrite(&mut self, block: &str, body: &str) -> bool {
    let texts = blocks::split(body);
    if block.is_empty() {
      self.blocks = blocks::assign(&self.blocks, texts, &mut self.made);
      return true;
    }
    let Some(place) = self.blocks.iter().position(|held| held.id == block) else { return false };
    let mut replaced = Vec::with_capacity(texts.len());
    for (i, text) in texts.into_iter().enumerate() {
      replaced.push(match i {
        0 => Block { id: block.to_owned(), text },
        _ => blocks::fresh(&mut self.made, text),
      });
    }
    self.blocks.splice(place..=place, replaced);
    true
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Draft {
  pub who: String,
  pub parent: String,
  pub anchor: String,
  pub body: String,
}

/// One blip or one block of it being rewritten. Holding it is the lock: the
/// second window to reach for something held is refused by there being an
/// entry already. `block` is empty for the whole blip, which is held against
/// every block of it, as each block is against the whole.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Editing {
  pub blip: String,
  pub block: String,
  pub who: String,
  pub body: String,
}

#[derive(Debug, Clone)]
pub struct Wave {
  pub id: String,
  pub title: String,
  pub participants: Vec<String>,
  pub blips: Vec<Blip>,
  /// Every durable change in the order the rules applied it: kept blips,
  /// amends, moves and votes, never a keystroke or a hold. The rest of the
  /// wave is this log applied to an empty wave. Held in memory only.
  pub log: Vec<Change>,
}

/// One durable change to a wave, carrying everything the rules need to apply
/// it again, so replaying the log rebuilds the wave down to its block ids.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Change {
  Kept { id: u64, parent: String, anchor: String, who: String, body: String, at: String },
  /// A rewrite of block `block` of a blip or of all of it when `block` is empty.
  Amended { blip: String, block: String, who: String, body: String, at: String },
  /// A move on the board that gadget block `block` of blip `blip` is.
  Played { blip: String, block: String, who: String, cell: usize, at: String },
  /// A fresh board.
  Cleared { blip: String, block: String, who: String, at: String },
  /// An answer to the vote that gadget block `block` of blip `blip` is or
  /// the same answer again to take it back.
  Voted { blip: String, block: String, who: String, answer: String, at: String },
}

impl Wave {
  /// A wave with nothing in it yet.
  pub fn new(id: &str, title: &str) -> Self {
    Self { id: id.to_owned(), title: title.to_owned(), participants: Vec::new(), blips: Vec::new(), log: Vec::new() }
  }

  /// Applies `change` and logs it. A change the rules refuse, a move out of
  /// turn or an amend of a block that is not there, changes nothing and is
  /// not logged.
  pub fn apply(&mut self, change: Change) -> bool {
    let applied = self.step(&change);
    if applied {
      self.log.push(change);
    }
    applied
  }

  /// The wave as it stood after its first `steps` changes.
  pub fn replayed(&self, steps: usize) -> Wave {
    let mut wave = Wave::new(&self.id, &self.title);
    for change in self.log.iter().take(steps) {
      wave.apply(change.clone());
    }
    wave
  }

  fn step(&mut self, change: &Change) -> bool {
    match change {
      Change::Kept { id, parent, anchor, who, body, at } => {
        self.blips.push(Blip::written(*id, parent, anchor, who, body, at.clone()));
        admit(self, who);
        true
      }
      Change::Amended { blip, block, who, body, at } => {
        let Some(amended) = self.blips.iter_mut().find(|held| held.id.to_string() == *blip) else { return false };
        if !amended.rewrite(block, body) {
          return false;
        }
        let blocks = &amended.blocks;
        amended.gadgets.retain(|id, state| blocks.iter().find(|held| held.id == *id).and_then(|held| gadgets::kind_of(&held.text)).is_some_and(|kind| kind.fits(state)));
        amended.edited = at.clone();
        if !who.is_empty() && !amended.editors.contains(who) {
          amended.editors.push(who.clone());
        }
        admit(self, who);
        true
      }
      Change::Played { blip, block, who, cell, .. } => self.gadget(blip, block, |_, state| match state {
        State::Board(game) => game.play(who, *cell),
        State::Votes(_) => false,
      }),
      Change::Cleared { blip, block, .. } => self.gadget(blip, block, |_, state| match state {
        State::Board(game) => {
          let played = *game != Game::default();
          game.clear();
          played
        }
        State::Votes(_) => false,
      }),
      Change::Voted { blip, block, who, answer, .. } => self.gadget(blip, block, |kind, state| match state {
        State::Votes(votes) => gadgets::vote(kind, votes, who, answer),
        State::Board(_) => false,
      }),
    }
  }

  /// Applies `change` to the state of gadget block `block` of blip `blip`,
  /// starting from a fresh one when it has none of its kind. The state is
  /// kept only when the change applied. False when the block is no gadget.
  fn gadget(&mut self, blip: &str, block: &str, change: impl FnOnce(&Kind, &mut State) -> bool) -> bool {
    let Some(held) = self.blips.iter_mut().find(|held| held.id.to_string() == blip) else { return false };
    let Some(kind) = held.blocks.iter().find(|held| held.id == block).and_then(|held| gadgets::kind_of(&held.text)) else { return false };
    let mut state = held.gadgets.get(block).filter(|state| kind.fits(state)).cloned().unwrap_or_else(|| kind.fresh());
    let applied = change(&kind, &mut state);
    if applied {
      held.gadgets.insert(block.to_owned(), state);
    }
    applied
  }
}

/// Everything about every wave, live and durable, owned by one controller and
/// mutated from one task. Nothing here is behind a lock: the rules are the
/// only writer and they run one input at a time.
#[derive(Debug, Default)]
pub struct Field {
  pub waves: BTreeMap<String, Wave>,
  /// Wave, then connection, then the name that window is using.
  pub here: BTreeMap<String, BTreeMap<Conn, String>>,
  /// Wave, then connection, then what that window is part way through typing.
  pub drafts: BTreeMap<String, BTreeMap<Conn, Draft>>,
  /// Wave, then blip and block, then the window rewriting it and how far it
  /// has got.
  pub edits: BTreeMap<String, BTreeMap<(String, String), (Conn, Editing)>>,
  /// Which wave a connection is looking at.
  pub watching: BTreeMap<Conn, String>,
  /// Waves whose drafts or rewrites changed since the last tick, when views
  /// wait for one.
  pub unsent: BTreeSet<String>,
  next: u64,
}

/// What one window is shown: who is here and what everyone *else* is typing.
/// Built per recipient, which is why a reader never sees a ghost of their own.
/// A window with no name is on the wave and in nothing else: it reads and the
/// rules drop every op it sends, so nobody can write anonymously.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct View {
  pub here: Vec<String>,
  pub drafts: Vec<Draft>,
  /// Every blip somebody else is rewriting, with the text as it stands. A
  /// window is never shown its own, the way it is never shown its own draft.
  pub edits: Vec<Editing>,
}

/// Every change to a wave, as a value. `View` is the one the server sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
  Watch { wave: String, name: String },
  Typing { parent: String, anchor: String, body: String },
  Keep { wave: String, parent: String, anchor: String, who: String, body: String },
  /// Take a block of a blip to rewrite it, the whole blip when `block` is
  /// empty. Refused by doing nothing when someone else holds it; the view is
  /// what tells both windows who won.
  Open { blip: String, block: String },
  Rewriting { blip: String, block: String, body: String },
  Close { blip: String, block: String },
  /// The rewrite, kept. An action submits this, never a socket, because a
  /// blip is durable.
  Amend { wave: String, blip: String, block: String, who: String, body: String },
  /// A move on a board gadget; a fresh board when `cell` is absent.
  Play { wave: String, blip: String, block: String, who: String, cell: Option<usize> },
  /// An answer to a vote gadget or the same answer again to take it back.
  Vote { wave: String, blip: String, block: String, who: String, answer: String },
  View(Box<View>),
}

impl Field {
  pub fn new(waves: Vec<Wave>) -> Self {
    let next = waves.iter().flat_map(|wave| wave.blips.iter().map(|blip| blip.id + 1)).max().unwrap_or(1);
    Self { waves: waves.into_iter().map(|wave| (wave.id.clone(), wave)).collect(), next, ..Self::default() }
  }

  /// The wave a connection is looking at, with everyone on it, which is who a
  /// change has to reach.
  fn audience(&self, wave: &str) -> Vec<Agent<Conn>> {
    self.here.get(wave).map(|here| here.keys().map(|conn| Agent::Human(*conn)).collect()).unwrap_or_default()
  }

  fn forget(&mut self, conn: Conn) -> Option<String> {
    let wave = self.watching.remove(&conn)?;
    if let Some(here) = self.here.get_mut(&wave) {
      here.remove(&conn);
    }
    if let Some(drafts) = self.drafts.get_mut(&wave) {
      drafts.remove(&conn);
    }
    if let Some(edits) = self.edits.get_mut(&wave) {
      edits.retain(|_, (held_by, _)| *held_by != conn);
    }
    Some(wave)
  }

  /// Whether `conn` is the window holding `block` of `blip`. Nothing else may write it.
  fn holds(&self, wave: &str, blip: &str, block: &str, conn: Conn) -> bool {
    self.edits.get(wave).and_then(|edits| edits.get(&(blip.to_owned(), block.to_owned()))).is_some_and(|(held_by, _)| *held_by == conn)
  }

  /// Whether `block` of `blip` can be taken: nobody holds it or the whole
  /// blip and, for the whole blip, nobody holds any block of it.
  fn free(&self, wave: &str, blip: &str, block: &str) -> bool {
    let Some(edits) = self.edits.get(wave) else { return true };
    edits.keys().all(|(held, part)| held != blip || (!block.is_empty() && !part.is_empty() && part != block))
  }

  /// What a window starts a rewrite from: the whole blip's markdown or one block's.
  fn start_of(&self, wave: &str, blip: &str, block: &str) -> Option<String> {
    let held = self.waves.get(wave)?.blips.iter().find(|held| held.id.to_string() == blip)?;
    match block.is_empty() {
      true => Some(held.body()),
      false => held.blocks.iter().find(|held| held.id == block).map(|held| held.text.clone()),
    }
  }

  /// The rewrite kept, with its hold released alongside it.
  fn amend(&mut self, wave: &str, blip: &str, block: &str, who: &str, body: &str, at: String) {
    if let Some(edits) = self.edits.get_mut(wave) {
      edits.remove(&(blip.to_owned(), block.to_owned()));
    }
    if let Some(held) = self.waves.get_mut(wave) {
      held.apply(Change::Amended { blip: blip.to_owned(), block: block.to_owned(), who: who.to_owned(), body: body.to_owned(), at });
    }
  }

  fn keep(&mut self, wave: &str, parent: &str, anchor: &str, who: &str, body: &str, at: String) {
    let id = self.next;
    let Some(held) = self.waves.get_mut(wave) else { return };
    if held.apply(Change::Kept { id, parent: parent.to_owned(), anchor: anchor.to_owned(), who: who.to_owned(), body: body.to_owned(), at }) {
      self.next += 1;
    }
  }

  /// What `wave` looks like to `conn`: everyone here and every draft and
  /// rewrite but this window's own.
  pub fn view(&self, wave: &str, conn: Option<Conn>) -> View {
    let mut here: Vec<String> = self.here.get(wave).map(|here| here.values().filter(|name| !name.is_empty()).cloned().collect()).unwrap_or_default();
    here.sort();
    here.dedup();
    let drafts = self
      .drafts
      .get(wave)
      .map(|drafts| drafts.iter().filter(|(at, _)| Some(**at) != conn).map(|(_, draft)| draft.clone()).collect())
      .unwrap_or_default();
    let edits = self
      .edits
      .get(wave)
      .map(|edits| edits.values().filter(|(held_by, _)| Some(*held_by) != conn).map(|(_, edit)| edit.clone()).collect())
      .unwrap_or_default();
    View { here, drafts, edits }
  }
}

/// The rules. The only place the field changes.
pub struct Rules {
  /// What a kept blip is stamped with, so a test can hold the clock still.
  pub clock: Box<dyn Fn() -> String + Send + Sync>,
  /// A keystroke (`Typing` or `Rewriting`) waits for the next `TimeStep` to
  /// be sent, so a wave sends views at the tick rate however fast anyone
  /// types. Every other change is sent at once. Needs a `TickDriver`.
  pub on_tick: bool,
  /// One view for everyone on the wave, carrying every draft including the
  /// recipient's own, instead of one per window without it.
  pub uniform: bool,
}

impl Rules {
  pub fn new() -> Self {
    Self { clock: Box::new(now), on_tick: false, uniform: false }
  }

  /// Where a keystroke's wave goes: out with this input or into `unsent` for
  /// the next tick.
  fn keystroke(&self, field: &mut Field, touched: &mut Option<String>, wave: String) {
    match self.on_tick {
      true => {
        field.unsent.insert(wave);
      }
      false => *touched = Some(wave),
    }
  }
}

impl Default for Rules {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl StateLogic<Op, Conn, Field> for Rules {
  async fn process_input(&self, field: &mut Field, input: LogicInput<Op, Conn>) -> Result<LogicOutput<Op, Conn>, StateLogicError> {
    match input {
      LogicInput::AgentOps { source, ops } => {
        let conn = source.id_cloned();
        let mut touched: Option<String> = None;
        for op in ops {
          match op {
            Op::Watch { wave, name } => {
              let Some(conn) = conn else { continue };
              if !field.waves.contains_key(&wave) {
                return Err(StateLogicError::InvalidOperation(format!("no wave `{wave}`")));
              }
              field.watching.insert(conn, wave.clone());
              field.here.entry(wave.clone()).or_default().insert(conn, name);
              touched = Some(wave);
            }
            Op::Typing { parent, anchor, body } => {
              let Some(conn) = conn else { continue };
              let Some(wave) = field.watching.get(&conn).cloned() else { continue };
              let who = field.here.get(&wave).and_then(|here| here.get(&conn)).cloned().unwrap_or_default();
              if who.is_empty() {
                continue;
              }
              let drafts = field.drafts.entry(wave.clone()).or_default();
              match body.is_empty() {
                true => {
                  drafts.remove(&conn);
                }
                false => {
                  drafts.insert(conn, Draft { who, parent, anchor, body });
                }
              }
              self.keystroke(field, &mut touched, wave);
            }
            Op::Keep { wave, parent, anchor, who, body } => {
              field.keep(&wave, &parent, &anchor, &who, &body, (self.clock)());
              if let (Some(conn), Some(drafts)) = (conn, field.drafts.get_mut(&wave)) {
                drafts.remove(&conn);
              }
              touched = Some(wave);
            }
            Op::Open { blip, block } => {
              let Some(conn) = conn else { continue };
              let Some(wave) = field.watching.get(&conn).cloned() else { continue };
              if !field.free(&wave, &blip, &block) {
                continue;
              }
              let Some(body) = field.start_of(&wave, &blip, &block) else { continue };
              let who = field.here.get(&wave).and_then(|here| here.get(&conn)).cloned().unwrap_or_default();
              if who.is_empty() {
                continue;
              }
              field.edits.entry(wave.clone()).or_default().insert((blip.clone(), block.clone()), (conn, Editing { blip, block, who, body }));
              touched = Some(wave);
            }
            Op::Rewriting { blip, block, body } => {
              let Some(conn) = conn else { continue };
              let Some(wave) = field.watching.get(&conn).cloned() else { continue };
              if !field.holds(&wave, &blip, &block, conn) {
                continue;
              }
              if let Some((_, edit)) = field.edits.get_mut(&wave).and_then(|edits| edits.get_mut(&(blip, block))) {
                edit.body = body;
              }
              self.keystroke(field, &mut touched, wave);
            }
            Op::Close { blip, block } => {
              let Some(conn) = conn else { continue };
              let Some(wave) = field.watching.get(&conn).cloned() else { continue };
              if !field.holds(&wave, &blip, &block, conn) {
                continue;
              }
              if let Some(edits) = field.edits.get_mut(&wave) {
                edits.remove(&(blip, block));
              }
              touched = Some(wave);
            }
            Op::Amend { wave, blip, block, who, body } => {
              field.amend(&wave, &blip, &block, &who, &body, (self.clock)());
              touched = Some(wave);
            }
            Op::Play { wave, blip, block, who, cell } => {
              if let Some(held) = field.waves.get_mut(&wave) {
                let at = (self.clock)();
                held.apply(match cell {
                  Some(cell) => Change::Played { blip, block, who, cell, at },
                  None => Change::Cleared { blip, block, who, at },
                });
              }
              touched = Some(wave);
            }
            Op::Vote { wave, blip, block, who, answer } => {
              if let Some(held) = field.waves.get_mut(&wave) {
                held.apply(Change::Voted { blip, block, who, answer, at: (self.clock)() });
              }
              touched = Some(wave);
            }
            Op::View(_) => {}
          }
        }
        Ok(views(LogicOutput::none(), field, touched, self.uniform))
      }
      LogicInput::AgentLeft { agent_id } => {
        let wave = field.forget(agent_id);
        Ok(views(LogicOutput::none(), field, wave, self.uniform))
      }
      LogicInput::TimeStep { .. } => {
        let unsent = std::mem::take(&mut field.unsent);
        Ok(unsent.into_iter().fold(LogicOutput::none(), |out, wave| views(out, field, Some(wave), self.uniform)))
      }
      LogicInput::AgentJoined { .. } => Ok(LogicOutput::none()),
    }
  }
}

/// Everyone on the wave that changed is sent a view: one each or one shared
/// when `uniform`. A change to no wave sends nothing.
/// Whoever keeps or rewrites a blip is on the wave from then on, as they were in Wave.
fn admit(wave: &mut Wave, who: &str) {
  if !who.is_empty() && !wave.participants.iter().any(|there| there == who) {
    wave.participants.push(who.to_owned());
  }
}

fn views(out: LogicOutput<Op, Conn>, field: &Field, wave: Option<String>, uniform: bool) -> LogicOutput<Op, Conn> {
  let Some(wave) = wave else { return out };
  let audience = field.audience(&wave);
  if audience.is_empty() {
    return out;
  }
  out.and_snapshot(match uniform {
    true => SnapshotRequest::uniform_with_context(audience, SnapshotContext::ForPerspective(wave)),
    false => SnapshotRequest::to(audience),
  })
}

/// The view of a wave: built once per recipient or once for a uniform request
/// from the wave it names.
pub struct Views;

#[async_trait]
impl SnapshotProvider<Conn, Field, Op> for Views {
  async fn create_snapshot(
    &self,
    field: &Field,
    target: Option<&Agent<Conn>>,
    context: Option<SnapshotContext>,
  ) -> Result<Option<Op>, SnapshotError<Conn>> {
    match (target.and_then(|agent| agent.id_cloned()), context) {
      (Some(conn), _) => {
        let Some(wave) = field.watching.get(&conn) else { return Ok(None) };
        Ok(Some(Op::View(Box::new(field.view(wave, Some(conn))))))
      }
      (None, Some(SnapshotContext::ForPerspective(wave))) => Ok(Some(Op::View(Box::new(field.view(&wave, None))))),
      (None, _) => Ok(None),
    }
  }
}

pub fn now() -> String {
  let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
  format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}
