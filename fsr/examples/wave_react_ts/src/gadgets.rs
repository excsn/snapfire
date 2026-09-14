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

/// When voting on the gadget `text` is ends: the `until=` word of its fence's
/// info string, a UTC minute written `YYYY-MM-DDTHH:MMZ`, in seconds since the
/// epoch. None for no deadline or one that does not read.
pub fn until(text: &str) -> Option<u64> {
  let mut events = Parser::new_ext(text, Options::empty());
  let Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))) = events.next() else { return None };
  info.split_whitespace().find_map(|word| word.strip_prefix("until=")).and_then(utc_minute)
}

/// `YYYY-MM-DDTHH:MMZ` in seconds since the epoch.
pub fn utc_minute(text: &str) -> Option<u64> {
  let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
  let mut date = date.split('-').map(str::parse::<i64>);
  let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
  let (hour, minute) = time.split_once(':')?;
  let (hour, minute) = (hour.parse::<i64>().ok()?, minute.parse::<i64>().ok()?);
  if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(0..24).contains(&hour) || !(0..60).contains(&minute) {
    return None;
  }
  u64::try_from(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60).ok()
}

/// Seconds since the epoch as `15 Sep 18:00 UTC`.
pub fn shown_utc(secs: u64) -> String {
  const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  let (_, month, day) = civil_from_days((secs / 86_400) as i64);
  format!("{day} {} {:02}:{:02} UTC", MONTHS[(month - 1) as usize], (secs / 3600) % 24, (secs / 60) % 60)
}

/// Days since 1970-01-01 of a proleptic Gregorian date: Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
  let year = if month <= 2 { year - 1 } else { year };
  let era = year.div_euclid(400);
  let yoe = year - era * 400;
  let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146_097 + doe - 719_468
}

/// The year, month and day `days` after 1970-01-01: Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = z - era * 146_097;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let day = doy - (153 * mp + 2) / 5 + 1;
  let month = if mp < 10 { mp + 3 } else { mp - 9 };
  (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// What a vote came to, as a sentence: the answer with the most votes, every
/// answer tied for the most or nobody at all.
pub fn tally(kind: &Kind, votes: &BTreeMap<String, String>) -> String {
  let counts: Vec<(String, usize)> = kind
    .answers()
    .into_iter()
    .map(|answer| {
      let given = votes.values().filter(|given| **given == answer).count();
      (answer, given)
    })
    .collect();
  let total: usize = counts.iter().map(|(_, given)| given).sum();
  let most = counts.iter().map(|(_, given)| *given).max().unwrap_or(0);
  if most == 0 {
    return "Nobody voted.".to_owned();
  }
  let noun = |n: usize| if n == 1 { "vote" } else { "votes" };
  let top: Vec<&str> = counts.iter().filter(|(_, given)| *given == most).map(|(answer, _)| answer.as_str()).collect();
  match top.as_slice() {
    [one] => format!("{one} won with {most} of {total} {}.", noun(total)),
    _ => format!("{} tied with {most} {} each.", listed(&top), noun(most)),
  }
}

/// `a`, `a and b`, `a, b and c`.
fn listed(names: &[&str]) -> String {
  match names.split_last() {
    Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
    Some((last, _)) => (*last).to_owned(),
    None => String::new(),
  }
}

/// The body of the blip announcing a closed vote: its question as a link to
/// blip `blip`, which holds it, then what it came to as a paragraph of its own.
pub fn announcement(kind: &Kind, votes: &BTreeMap<String, String>, blip: &str) -> String {
  let question = match kind.question() {
    "" => "the poll".to_owned(),
    question => question.replace('[', "\\[").replace(']', "\\]"),
  };
  format!("Poll closed: [{question}](#blip-{blip})\n\n{}", tally(kind, votes))
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

  #[test]
  fn a_deadline_is_a_utc_minute_on_the_fence_s_info_string() {
    assert_eq!(utc_minute("2000-03-01T00:00Z"), Some(951_868_800));
    assert_eq!(until("```gadget poll until=2000-03-01T00:01Z\nWhen?\nThursday\n```"), Some(951_868_860));
    assert_eq!(kind_of("```gadget poll until=2000-03-01T00:01Z\nWhen?\nThursday\n```").map(|kind| kind.answers()), Some(vec!["Thursday".to_owned()]), "the deadline is no answer");
    assert_eq!(until("```gadget poll\nWhen?\n```"), None, "a poll without one runs until its author closes it");
    assert_eq!(until("```gadget poll until=soon\nWhen?\n```"), None, "and one that does not read is none");
    assert_eq!(utc_minute("2026-02-30T25:00Z"), None);
    assert_eq!(shown_utc(utc_minute("2026-09-15T18:05Z").unwrap()), "15 Sep 18:05 UTC");
    assert_eq!(shown_utc(utc_minute("2024-02-29T00:00Z").unwrap()), "29 Feb 00:00 UTC", "a leap day");
  }

  #[test]
  fn a_closed_vote_is_announced_by_its_result_with_a_link_back() {
    let poll = Kind::Poll { question: "When [roughly]?".to_owned(), choices: ["Thursday", "Friday", "Next week"].map(str::to_owned).to_vec() };
    let votes = |pairs: &[(&str, &str)]| pairs.iter().map(|(who, answer)| ((*who).to_owned(), (*answer).to_owned())).collect::<BTreeMap<_, _>>();
    assert_eq!(announcement(&poll, &votes(&[("alice", "Friday"), ("bob", "Friday"), ("carol", "Thursday")]), "8"), "Poll closed: [When \\[roughly\\]?](#blip-8)\n\nFriday won with 2 of 3 votes.");
    assert_eq!(tally(&poll, &votes(&[("alice", "Friday")])), "Friday won with 1 of 1 vote.");
    assert_eq!(tally(&poll, &votes(&[("alice", "Friday"), ("bob", "Thursday"), ("carol", "Next week")])), "Thursday, Friday and Next week tied with 1 vote each.");
    assert_eq!(tally(&poll, &votes(&[])), "Nobody voted.");
    assert_eq!(announcement(&Kind::YesNo { question: String::new() }, &votes(&[("alice", "No")]), "9"), "Poll closed: [the poll](#blip-9)\n\nNo won with 1 of 1 vote.");
  }
}
