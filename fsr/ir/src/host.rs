//! The names a lowered page and the program that runs it share. A handler
//! is named for the event that runs it, an [`Event`]. When a handler needs
//! something only the program can do, a timer, a spawned process or a
//! promise settled, it emits a `Stmt::Act` naming an [`Act`]. A spawned
//! process reports back through a [`Report`]. The terminal runs all of them.
//! The web host runs none. The lowerer refuses source that would need one
//! on a web page.

/// The key a promise's `resolve` or `reject` is held under: `{ "$settles": deferred, ok }`, `ok` false for `reject`. Calling one is an [`Act::Settle`]; a timer on one settles the deferred when it is due.
pub const SETTLES: &str = "$settles";

/// The key an act reference is held under, `{ "$act": action }`: what `action("$stdout")` is in a terminal page. A handler's call to one issues the act with what it was called with.
pub const ACT_REF: &str = "$act";

/// The act a value refers to: `{ "$act": action }`, read through any field of an object written out that holds one.
pub fn act_ref(expr: &crate::Expr) -> Option<String> {
  use crate::ast::{Entry, Expr, Lit};
  match expr {
    Expr::Object(entries) => entries.iter().find_map(|entry| match entry {
      Entry::Field(key, Expr::Lit(Lit::Str(action))) if key == ACT_REF => Some(action.clone()),
      _ => None,
    }),
    Expr::Field(object, name) => match &**object {
      Expr::Object(entries) => entries.iter().rev().find_map(|entry| match entry {
        Entry::Field(key, value) if key == name => act_ref(value),
        _ => None,
      }),
      _ => None,
    },
    _ => None,
  }
}

/// The key a timer a handler started is held under, `{ "$timer": id }`, the id its [`Act::Timer`] names and [`Act::ClearTimer`] takes: what `setInterval` and `setTimeout` return in a handler.
pub const TIMER: &str = "$timer";

/// The key a process a block spawned is held under, `{ "$process": id }`, the id its [`Act::Spawn`] and its listeners name.
pub const PROCESS: &str = "$process";

/// The id [`PROCESS`] names on the terminal's own output, `useStdout().stdout`, which reports [`Report::Resize`].
pub const TERMINAL_OUTPUT: &str = "$terminal";

/// What a terminal runs a handler on, the event its `Handler::event` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
  /// A key, Ink's `useInput`; the event is `{ input, key }`.
  Input,
  /// The component's first render.
  Mount,
  /// Every so many milliseconds while the component renders.
  Interval(u64),
  /// Once, so many milliseconds after the component first renders.
  Timeout(u64),
  /// The function a mount effect declares in this place, which a [`Act::Timer`] starts again.
  Call(usize),
  /// A report from the process a mount effect spawned as `id`.
  Process { id: String, report: Report },
  /// The effect in this place whose dependencies change, which a terminal runs after a render that changed them.
  Effect(usize),
  /// The rest of an async function after the `await` the key names, `file@offset`, run when the awaited promise settles with `{ value, ok, carry }`.
  Resume(String),
}

impl Event {
  pub fn name(&self) -> String {
    match self {
      Event::Input => "input".to_owned(),
      Event::Mount => "mount".to_owned(),
      Event::Interval(ms) => format!("interval:{ms}"),
      Event::Timeout(ms) => format!("timeout:{ms}"),
      Event::Call(n) => format!("call:{n}"),
      Event::Process { id, report } => format!("spawn:{id}:{}", report.name()),
      Event::Effect(n) => format!("effect:{n}"),
      Event::Resume(at) => format!("resume:{at}"),
    }
  }

  pub fn parse(name: &str) -> Option<Event> {
    match name {
      "input" => return Some(Event::Input),
      "mount" => return Some(Event::Mount),
      _ => {}
    }
    let (kind, rest) = name.split_once(':')?;
    match kind {
      "interval" => rest.parse().ok().map(Event::Interval),
      "timeout" => rest.parse().ok().map(Event::Timeout),
      "call" => rest.parse().ok().map(Event::Call),
      "effect" => rest.parse().ok().map(Event::Effect),
      "resume" => Some(Event::Resume(rest.to_owned())),
      "spawn" => {
        let (id, report) = rest.rsplit_once(':')?;
        Some(Event::Process { id: id.to_owned(), report: Report::parse(report)? })
      }
      _ => None,
    }
  }
}

/// What a spawned process reports: a chunk of `stdout` or `stderr` as text, `exit` and `close` with `{ code, signal }`, `error` with `{ message }`; `resize` with `null`, which only [`TERMINAL_OUTPUT`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
  Stdout,
  Stderr,
  Exit,
  Close,
  Error,
  Resize,
}

impl Report {
  pub fn name(self) -> &'static str {
    match self {
      Report::Stdout => "stdout",
      Report::Stderr => "stderr",
      Report::Exit => "exit",
      Report::Close => "close",
      Report::Error => "error",
      Report::Resize => "resize",
    }
  }

  pub fn parse(name: &str) -> Option<Report> {
    Some(match name {
      "stdout" => Report::Stdout,
      "stderr" => Report::Stderr,
      "exit" => Report::Exit,
      "close" => Report::Close,
      "error" => Report::Error,
      "resize" => Report::Resize,
      _ => return None,
    })
  }
}

/// What a terminal handler asks its terminal for, the action its `Stmt::Act` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
  /// A line of text written above the page, `useStdout().write`.
  Stdout,
  /// The same through `useStderr().write`.
  Stderr,
  /// `{ event, ms, repeats }`: a timer on the handler [`Event::Call`] names; `{ id, event, carry, ms, repeats }`, one on a continuation run with `{ value: null, ok: true, carry }`, which [`Act::ClearTimer`] stops by `id`; `{ settle, ms }`, one that settles a promise, [`SETTLES`].
  Timer,
  /// `{ timer }`: stop the timer [`TIMER`] names, `clearInterval` and `clearTimeout`.
  ClearTimer,
  /// `{ effect, event, carry }`: the cleanup the [`Event::Effect`] handler `effect` returned, run before that effect runs again.
  Cleanup,
  /// `{ id, command, args, options, stdin }`: a process to start, whose reports are [`Event::Process`]; with `stdin` true its standard input is a pipe [`Act::Stdin`] writes, else it reads nothing.
  Spawn,
  /// `{ id, text }`: write `text` to the standard input of the process `id` names; `{ id, end: true }`: close it.
  Stdin,
  /// `{ settle, value }`: a promise's `resolve` or `reject`, [`SETTLES`], called with `value`.
  Settle,
  /// `{ deferred, event, carry }`: run the handler `event` names when `deferred` settles, with `{ value, ok, carry }`. `{ rejected, event, carry }`: run it now with `ok` false and `rejected` as the value, for a call that failed as it was awaited.
  Then,
  /// `{ process, report, event, carry }` or `{ process, report, settle }`: when the process [`PROCESS`] names reports `report`, run the handler `event` names with `{ value, ok, carry }`. With `settle` instead, settle the promise it names, [`SETTLES`], with what it reported.
  Listen,
  /// `{ process, report, event }`: stop running the handler `event` names when the process reports `report`, `off(event, f)`.
  Unlisten,
  /// `{ event, carry }`: run the handler `event` names next, with `{ value: null, ok: true, carry }`; where the ways through an async handler's `if` or `try` join.
  Run,
  /// `{ until, settles }`: give up the screen until the deferred `until` settles, then take it back and settle `settles` as `until` settled. Ink's `suspendTerminal`.
  Suspend,
  /// Ink's `exit()`: the page is done and the terminal quits.
  Exit,
  /// `focus(id)`.
  Focus,
  FocusNext,
  FocusPrevious,
  FocusEnable,
  FocusDisable,
}

impl Act {
  pub fn name(self) -> &'static str {
    match self {
      Act::Stdout => "$stdout",
      Act::Stderr => "$stderr",
      Act::Timer => "$timer",
      Act::ClearTimer => "$clearTimer",
      Act::Cleanup => "$cleanup",
      Act::Spawn => "$spawn",
      Act::Stdin => "$stdin",
      Act::Settle => "$settle",
      Act::Then => "$then",
      Act::Suspend => "$suspend",
      Act::Listen => "$listen",
      Act::Unlisten => "$unlisten",
      Act::Run => "$run",
      Act::Exit => "$exit",
      Act::Focus => "$focus",
      Act::FocusNext => "$focusNext",
      Act::FocusPrevious => "$focusPrevious",
      Act::FocusEnable => "$focusEnable",
      Act::FocusDisable => "$focusDisable",
    }
  }

  pub fn parse(name: &str) -> Option<Act> {
    [Act::Stdout, Act::Stderr, Act::Timer, Act::ClearTimer, Act::Cleanup, Act::Spawn, Act::Stdin, Act::Settle, Act::Then, Act::Suspend, Act::Listen, Act::Unlisten, Act::Run, Act::Exit, Act::Focus, Act::FocusNext, Act::FocusPrevious, Act::FocusEnable, Act::FocusDisable].into_iter().find(|act| act.name() == name)
  }
}

/// The event a mount timer's handler is named for.
pub fn timer(repeats: bool, ms: u64) -> Event {
  match repeats {
    true => Event::Interval(ms),
    false => Event::Timeout(ms),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn each_event_reads_back_from_its_name() {
    for event in [Event::Input, Event::Mount, Event::Interval(150), Event::Timeout(0), Event::Call(3), Event::Effect(2), Event::Resume("routes/page.tsx@41".to_owned()), Event::Process { id: "41".to_owned(), report: Report::Stdout }, Event::Process { id: "41".to_owned(), report: Report::Close }] {
      assert_eq!(Event::parse(&event.name()), Some(event));
    }
    assert_eq!(Event::parse("click"), None);
  }
}
