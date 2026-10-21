//! What a terminal page and the terminal that runs it agree on: the events a
//! handler is named for, the acts a handler asks for and the kinds of value a
//! body emits. Both sides build and read these names only through the types
//! here.

/// A focusable a terminal can move focus to, `{ id, autoFocus, isActive }`, one per `useFocus`, in render order.
pub const FOCUSABLE: &str = "focusable";

/// Where a terminal puts its cursor, `{ x, y }` from the corner of the output; null hides it. Ink's `setCursorPosition` sets it and the last one a step emits holds.
pub const CURSOR: &str = "cursor";

/// An effect whose dependencies change, `{ event, deps }`: the [`Event::Effect`] handler that runs it and the dependencies' values this render. A terminal runs the handler after a render whose values differ from the last ones it saw for that component, the first render included.
pub const EFFECT: &str = "effect";

/// The body `let` that emits a value of `kind`.
pub fn emit(kind: &str) -> String {
  format!("{}{kind}", snapfire_fsr_ir::render::EMIT_PREFIX)
}

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
      "spawn" => {
        let (id, report) = rest.rsplit_once(':')?;
        Some(Event::Process { id: id.to_owned(), report: Report::parse(report)? })
      }
      _ => None,
    }
  }
}

/// What a spawned process reports: a chunk of `stdout` or `stderr` as text, `exit` and `close` with `{ code, signal }`, `error` with `{ message }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
  Stdout,
  Stderr,
  Exit,
  Close,
  Error,
}

impl Report {
  pub fn name(self) -> &'static str {
    match self {
      Report::Stdout => "stdout",
      Report::Stderr => "stderr",
      Report::Exit => "exit",
      Report::Close => "close",
      Report::Error => "error",
    }
  }

  pub fn parse(name: &str) -> Option<Report> {
    Some(match name {
      "stdout" => Report::Stdout,
      "stderr" => Report::Stderr,
      "exit" => Report::Exit,
      "close" => Report::Close,
      "error" => Report::Error,
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
  /// `{ event, ms, repeats }`: a timer on the handler [`Event::Call`] names.
  Timer,
  /// `{ id, command, args, options }`: a process to start, whose reports are [`Event::Process`].
  Spawn,
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
      Act::Spawn => "$spawn",
      Act::Focus => "$focus",
      Act::FocusNext => "$focusNext",
      Act::FocusPrevious => "$focusPrevious",
      Act::FocusEnable => "$focusEnable",
      Act::FocusDisable => "$focusDisable",
    }
  }

  pub fn parse(name: &str) -> Option<Act> {
    [Act::Stdout, Act::Stderr, Act::Timer, Act::Spawn, Act::Focus, Act::FocusNext, Act::FocusPrevious, Act::FocusEnable, Act::FocusDisable].into_iter().find(|act| act.name() == name)
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
    for event in [Event::Input, Event::Mount, Event::Interval(150), Event::Timeout(0), Event::Call(3), Event::Effect(2), Event::Process { id: "41".to_owned(), report: Report::Stdout }, Event::Process { id: "41".to_owned(), report: Report::Close }] {
      assert_eq!(Event::parse(&event.name()), Some(event));
    }
    assert_eq!(Event::parse("click"), None);
  }
}
