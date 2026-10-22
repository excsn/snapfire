//! What a terminal page and the terminal that runs it agree on: the events a
//! handler is named for, the acts a handler asks for and the kinds of value a
//! body emits. Both sides build and read these names only through the types
//! here.

/// Where a terminal page's Ink API is read from: [`INK_SOURCE`], provided to the build under this name.
pub const INK_FILE: &str = "$fsr/terminal.tsx";

/// Ink's hooks written over the terminal primitives, which the build reads wherever a page imports one of its exports from [`INK_MODULES`].
pub const INK_SOURCE: &str = include_str!("../terminal/ink.tsx");

/// The specifiers a page imports Ink's API from.
pub const INK_MODULES: [&str; 2] = ["ink", "@snapfire/fsr/terminal"];

/// The npm packages around Ink a terminal page may import, each with the file the build provides in its place and that file's source: `(specifier, file, source)`.
pub const COMPANIONS: [(&str, &str, &str); 2] = [
  ("cli-spinners", "$fsr/terminal/cli-spinners.ts", include_str!("../terminal/cli-spinners.ts")),
  ("ink-spinner", "$fsr/terminal/ink-spinner.tsx", include_str!("../terminal/ink-spinner.tsx")),
];

/// A focusable a terminal can move focus to, `{ id, autoFocus, isActive }`, one per `useFocus`, in render order.
pub const FOCUSABLE: &str = "focusable";

/// Where a terminal puts its cursor, `{ x, y }` from the corner of the output; null hides it. Ink's `setCursorPosition` sets it and the last one a step emits holds.
pub const CURSOR: &str = "cursor";

/// An effect whose dependencies change, `{ event, deps }`: the [`Event::Effect`] handler that runs it and the dependencies' values this render. A terminal runs the handler after a render whose values differ from the last ones it saw for that component, the first render included.
pub const EFFECT: &str = "effect";

/// The key a promise's `resolve` or `reject` is held under: `{ "$settles": deferred, ok }`, `ok` false for `reject`. Calling one is an [`Act::Settle`]; a timer on one settles the deferred when it is due.
pub const SETTLES: &str = "$settles";

/// The key an act reference is held under, `{ "$act": action }`: what `action("$stdout")` is in a terminal page. A handler's call to one issues the act with what it was called with.
pub const ACT_REF: &str = "$act";

/// The key a terminal primitive used as a value is held under, `{ "$primitive": name }`: `suspendTerminal` as Ink's `useApp` hands it back. A call through one is the primitive's.
pub const PRIMITIVE: &str = "$primitive";

/// The primitive a value refers to, read the way [`act_ref`] reads an act.
pub fn primitive_ref(expr: &snapfire_fsr_ir::Expr) -> Option<String> {
  use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
  match expr {
    Expr::Object(entries) => entries.iter().find_map(|entry| match entry {
      Entry::Field(key, Expr::Lit(Lit::Str(name))) if key == PRIMITIVE => Some(name.clone()),
      _ => None,
    }),
    Expr::Field(object, name) => match &**object {
      Expr::Object(entries) => entries.iter().rev().find_map(|entry| match entry {
        Entry::Field(key, value) if key == name => primitive_ref(value),
        _ => None,
      }),
      _ => None,
    },
    _ => None,
  }
}

/// The act a value refers to: `{ "$act": action }`, read through any field of an object written out that holds one.
pub fn act_ref(expr: &snapfire_fsr_ir::Expr) -> Option<String> {
  use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
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
  /// `{ id, command, args, options }`: a process to start, whose reports are [`Event::Process`].
  Spawn,
  /// `{ settle, value }`: a promise's `resolve` or `reject`, [`SETTLES`], called with `value`.
  Settle,
  /// `{ deferred, event, carry }`: run the handler `event` names when `deferred` settles, with `{ value, ok, carry }`.
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
    [Act::Stdout, Act::Stderr, Act::Timer, Act::ClearTimer, Act::Cleanup, Act::Spawn, Act::Settle, Act::Then, Act::Suspend, Act::Listen, Act::Unlisten, Act::Run, Act::Exit, Act::Focus, Act::FocusNext, Act::FocusPrevious, Act::FocusEnable, Act::FocusDisable].into_iter().find(|act| act.name() == name)
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
