//! Gadgets: state the people on a wave share inside a blip. A gadget is a
//! fenced block whose info string is `gadget <kind>`, so it is a block like any
//! other, with an id that stays with it while the text around it changes.

use std::collections::{BTreeMap, BTreeSet};

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

/// What a gadget block is, read from its fence.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
  /// `gadget noughts`: a board of noughts and crosses.
  Noughts,
  /// `gadget yesno`: one answer each of yes, no or maybe to the fence's first line.
  YesNo { question: String },
  /// `gadget poll`: the fence's first line is the question and every line after it a choice.
  Poll { question: String, choices: Vec<String> },
}

/// The gadget `text` is: one fence whose info string is `gadget` and a kind
/// this knows. A fence of code, a kind nobody wrote or a fence with text
/// around it is not a gadget.
pub fn kind_of(text: &str) -> Option<Kind> {
  let mut events = Parser::new_ext(text, Options::empty());
  let Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))) = events.next() else { return None };
  let mut words = info.split_whitespace();
  if words.next() != Some("gadget") {
    return None;
  }
  let name = words.next()?.to_owned();
  let mut body = String::new();
  for event in events.by_ref() {
    match event {
      Event::Text(text) => body.push_str(&text),
      Event::End(_) => break,
      _ => return None,
    }
  }
  if events.next().is_some() {
    return None;
  }
  let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned);
  match name.as_str() {
    "noughts" => Some(Kind::Noughts),
    "yesno" => Some(Kind::YesNo { question: lines.next().unwrap_or_default() }),
    "poll" => {
      let question = lines.next().unwrap_or_default();
      let mut seen = BTreeSet::new();
      Some(Kind::Poll { question, choices: lines.filter(|choice| seen.insert(choice.clone())).collect() })
    }
    _ => None,
  }
}

impl Kind {
  /// The name its fence gives it.
  pub fn name(&self) -> &'static str {
    match self {
      Kind::Noughts => "noughts",
      Kind::YesNo { .. } => "yesno",
      Kind::Poll { .. } => "poll",
    }
  }

  /// The question a vote asks; empty for a board.
  pub fn question(&self) -> &str {
    match self {
      Kind::Noughts => "",
      Kind::YesNo { question } | Kind::Poll { question, .. } => question,
    }
  }

  /// The answers a vote offers, in order; none for a board.
  pub fn answers(&self) -> Vec<String> {
    match self {
      Kind::Noughts => Vec::new(),
      Kind::YesNo { .. } => ["Yes", "No", "Maybe"].map(str::to_owned).to_vec(),
      Kind::Poll { choices, .. } => choices.clone(),
    }
  }

  /// The state a gadget of this kind starts from.
  pub fn fresh(&self) -> State {
    match self {
      Kind::Noughts => State::Board(Game::default()),
      Kind::YesNo { .. } | Kind::Poll { .. } => State::Votes(BTreeMap::new()),
    }
  }

  /// Whether `state` is a state of this kind. A board rewritten as a poll
  /// starts the poll from nothing.
  pub fn fits(&self, state: &State) -> bool {
    matches!((self, state), (Kind::Noughts, State::Board(_)) | (Kind::YesNo { .. } | Kind::Poll { .. }, State::Votes(_)))
  }
}

/// A gadget's state, which its blip keeps under the gadget block's id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum State {
  Board(Game),
  /// Each person's answer, by name.
  Votes(BTreeMap<String, String>),
}

/// `who` gives `answer` or takes their answer back by giving it again. False
/// for somebody with no name or an answer the gadget does not offer.
pub fn vote(kind: &Kind, votes: &mut BTreeMap<String, String>, who: &str, answer: &str) -> bool {
  if who.is_empty() || !kind.answers().iter().any(|offered| offered == answer) {
    return false;
  }
  match votes.get(who).is_some_and(|given| given == answer) {
    true => votes.remove(who),
    false => votes.insert(who.to_owned(), answer.to_owned()),
  };
  true
}

/// Noughts and crosses, the gadget every Wave demo had. `cells` is nine of
/// `.`, `x` or `o`; `turn` is the mark to play next and `won` the mark that
/// has three, empty while nobody does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Game {
  pub cells: String,
  pub turn: String,
  pub won: String,
  /// Who plays which mark, in the order they first moved.
  pub players: BTreeMap<String, String>,
}

impl Default for Game {
  fn default() -> Self {
    Self { cells: ".........".to_owned(), turn: "x".to_owned(), won: String::new(), players: BTreeMap::new() }
  }
}

const LINES: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

impl Game {
  /// The mark `who` plays: theirs if they have one, else the mark whose turn
  /// it is when nobody holds it. A third person watches.
  fn mark_for(&self, who: &str) -> Option<String> {
    if let Some(mark) = self.players.get(who) {
      return Some(mark.clone());
    }
    let taken = self.players.values().any(|mark| mark == &self.turn);
    (!taken).then(|| self.turn.clone())
  }

  /// Everything a move is: whose turn, whether the cell is free, whether that
  /// finished it. All of it branches, which is why none of it is in a handler.
  pub fn play(&mut self, who: &str, cell: usize) -> bool {
    if !self.won.is_empty() || cell >= 9 {
      return false;
    }
    let Some(mark) = self.mark_for(who) else { return false };
    if mark != self.turn {
      return false;
    }
    let mut cells: Vec<char> = self.cells.chars().collect();
    if cells.get(cell) != Some(&'.') {
      return false;
    }
    cells[cell] = mark.chars().next().unwrap_or('x');
    self.cells = cells.iter().collect();
    self.players.insert(who.to_owned(), mark.clone());
    self.won = LINES
      .iter()
      .find(|line| line.iter().all(|i| cells[*i] == cells[line[0]] && cells[*i] != '.'))
      .map(|line| cells[line[0]].to_string())
      .unwrap_or_default();
    self.turn = match mark.as_str() {
      "x" => "o".to_owned(),
      _ => "x".to_owned(),
    };
    true
  }

  pub fn clear(&mut self) {
    *self = Game::default();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_gadget_is_a_fence_whose_info_string_names_it() {
    assert_eq!(kind_of("```gadget noughts\n```"), Some(Kind::Noughts));
    assert_eq!(kind_of("~~~gadget yesno\nShip it?\n~~~"), Some(Kind::YesNo { question: "Ship it?".to_owned() }));
    let poll = Some(Kind::Poll { question: "When?".to_owned(), choices: vec!["Thursday".to_owned(), "Friday".to_owned()] });
    assert_eq!(kind_of("```gadget poll\nWhen?\nThursday\n\nFriday\nThursday\n```"), poll, "a blank line or a choice given twice counts once");
    assert_eq!(kind_of("```rust\nfn main() {}\n```"), None, "a fence of code is code");
    assert_eq!(kind_of("```gadget chess\n```"), None, "and so is a kind nobody wrote");
    assert_eq!(kind_of("Before.\n\n```gadget noughts\n```"), None, "a gadget is a block of its own");
  }

  #[test]
  fn a_vote_is_one_answer_each_and_giving_it_again_takes_it_back() {
    let kind = Kind::YesNo { question: String::new() };
    let mut votes = BTreeMap::new();
    assert!(vote(&kind, &mut votes, "alice", "Yes"));
    assert!(vote(&kind, &mut votes, "alice", "No"));
    assert_eq!(votes.get("alice").map(String::as_str), Some("No"), "a second answer replaces the first");
    assert!(vote(&kind, &mut votes, "alice", "No"));
    assert!(votes.is_empty(), "the same answer again takes it back");
    assert!(!vote(&kind, &mut votes, "alice", "Perhaps"), "an answer the gadget does not offer is refused");
    assert!(!vote(&kind, &mut votes, "", "Yes"), "as is one from nobody");
  }
}
