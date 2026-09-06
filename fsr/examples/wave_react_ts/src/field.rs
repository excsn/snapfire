use std::collections::BTreeMap;

use async_trait::async_trait;
use plaza::error::{SnapshotError, StateLogicError};
use plaza::snapshot::SnapshotContext;
use plaza::state_logic::{LogicOutput, SnapshotRequest};
use plaza::{Agent, LogicInput, SnapshotProvider, StateLogic};
use serde::{Deserialize, Serialize};

/// A connection, which is what plaza calls an agent here. Two windows of one
/// person are two agents, because presence and a draft belong to a window.
pub type Conn = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Blip {
  pub id: u64,
  pub parent: String,
  pub who: String,
  pub body: String,
  pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Draft {
  pub who: String,
  pub parent: String,
  pub body: String,
}

#[derive(Debug, Clone)]
pub struct Wave {
  pub id: String,
  pub title: String,
  pub participants: Vec<String>,
  pub blips: Vec<Blip>,
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
  /// Which wave a connection is looking at.
  pub watching: BTreeMap<Conn, String>,
  next: u64,
}

/// What one window is shown: who is here, and what everyone *else* is typing.
/// Built per recipient, which is why a reader never sees a ghost of their own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct View {
  pub here: Vec<String>,
  pub drafts: Vec<Draft>,
}

/// Every change to a wave, as a value. `View` is the one the server sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
  Watch { wave: String, name: String },
  Typing { parent: String, body: String },
  Keep { wave: String, parent: String, who: String, body: String },
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
    Some(wave)
  }

  fn keep(&mut self, wave: &str, parent: &str, who: &str, body: &str, at: String) -> Option<Blip> {
    let id = self.next;
    let held = self.waves.get_mut(wave)?;
    let blip = Blip { id, parent: parent.to_owned(), who: who.to_owned(), body: body.to_owned(), at };
    if !who.is_empty() && !held.participants.iter().any(|there| there == who) {
      held.participants.push(who.to_owned());
    }
    held.blips.push(blip.clone());
    self.next += 1;
    Some(blip)
  }

  /// What `wave` looks like to `conn`: everyone here, and every draft but
  /// this window's own.
  pub fn view(&self, wave: &str, conn: Option<Conn>) -> View {
    let mut here: Vec<String> = self.here.get(wave).map(|here| here.values().cloned().collect()).unwrap_or_default();
    here.sort();
    here.dedup();
    let drafts = self
      .drafts
      .get(wave)
      .map(|drafts| drafts.iter().filter(|(at, _)| Some(**at) != conn).map(|(_, draft)| draft.clone()).collect())
      .unwrap_or_default();
    View { here, drafts }
  }
}

/// The rules. The only place the field changes.
pub struct Rules {
  /// What a kept blip is stamped with, so a test can hold the clock still.
  pub clock: Box<dyn Fn() -> String + Send + Sync>,
}

impl Rules {
  pub fn new() -> Self {
    Self { clock: Box::new(now) }
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
            Op::Typing { parent, body } => {
              let Some(conn) = conn else { continue };
              let Some(wave) = field.watching.get(&conn).cloned() else { continue };
              let who = field.here.get(&wave).and_then(|here| here.get(&conn)).cloned().unwrap_or_default();
              let drafts = field.drafts.entry(wave.clone()).or_default();
              match body.is_empty() {
                true => {
                  drafts.remove(&conn);
                }
                false => {
                  drafts.insert(conn, Draft { who, parent, body });
                }
              }
              touched = Some(wave);
            }
            Op::Keep { wave, parent, who, body } => {
              field.keep(&wave, &parent, &who, &body, (self.clock)());
              if let (Some(conn), Some(drafts)) = (conn, field.drafts.get_mut(&wave)) {
                drafts.remove(&conn);
              }
              touched = Some(wave);
            }
            Op::View(_) => {}
          }
        }
        Ok(snapshot(field, touched))
      }
      LogicInput::AgentLeft { agent_id } => {
        let wave = field.forget(agent_id);
        Ok(snapshot(field, wave))
      }
      LogicInput::AgentJoined { .. } | LogicInput::TimeStep { .. } => Ok(LogicOutput::none()),
    }
  }
}

/// Everyone on the wave that changed is sent a view of their own; a change to
/// no wave sends nothing.
fn snapshot(field: &Field, wave: Option<String>) -> LogicOutput<Op, Conn> {
  match wave {
    Some(wave) => {
      let audience = field.audience(&wave);
      match audience.is_empty() {
        true => LogicOutput::none(),
        false => LogicOutput::none().and_snapshot(SnapshotRequest::to(audience)),
      }
    }
    None => LogicOutput::none(),
  }
}

/// The view of a wave, built once per recipient.
pub struct Views;

#[async_trait]
impl SnapshotProvider<Conn, Field, Op> for Views {
  async fn create_snapshot(
    &self,
    field: &Field,
    target: Option<&Agent<Conn>>,
    _context: Option<SnapshotContext>,
  ) -> Result<Option<Op>, SnapshotError<Conn>> {
    let Some(conn) = target.and_then(|agent| agent.id_cloned()) else { return Ok(None) };
    let Some(wave) = field.watching.get(&conn) else { return Ok(None) };
    Ok(Some(Op::View(Box::new(field.view(wave, Some(conn))))))
  }
}

pub fn now() -> String {
  let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
  format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}
