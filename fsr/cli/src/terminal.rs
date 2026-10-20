//! `fsr dev --terminal`: the application's root page in the terminal, drawn
//! with Ink's elements and stepped by its `useInput` handler. The page renders
//! `ink-*` markup through the interpreter, which the terminal crate reads back
//! and draws; a key runs the handler over the held state and only the cells
//! that changed are written. A change under the app lowers the page again and
//! keeps the state.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use notify::{RecursiveMode, Watcher};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, HandlerRef};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_terminal::{from_markup, render, Boxed, Color, Frame, Node, Text};

use crate::BuildError;

/// The page a terminal application draws.
pub const MODULE: &str = "routes/page.tsx#default";

/// How many lines the page wrote with `useStdout` or `useStderr` stay above it.
const WRITTEN: usize = 200;

/// How long the loop waits for a key when no timer is due sooner.
const IDLE: Duration = Duration::from_millis(100);

/// The flags Ink's `useInput` hands a handler beside the input text.
const KEYS: [&str; 14] = ["upArrow", "downArrow", "leftArrow", "rightArrow", "pageUp", "pageDown", "return", "escape", "ctrl", "shift", "tab", "backspace", "delete", "meta"];

/// What a keypress asks for.
pub enum Press {
  /// Ink's `(input, key)` as one value, `{ input, key }`.
  Input(Value),
  Exit,
}

/// A key as Ink's `useInput` reads it. `None` is a key it does not report.
/// Ctrl-C exits, as Ink's `exitOnCtrlC` does by default.
pub fn key_event(event: KeyEvent) -> Option<Press> {
  let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
  let mut on: Vec<&str> = Vec::new();
  if ctrl {
    on.push("ctrl");
  }
  if event.modifiers.contains(KeyModifiers::SHIFT) {
    on.push("shift");
  }
  if event.modifiers.contains(KeyModifiers::ALT) {
    on.push("meta");
  }
  let input = match event.code {
    KeyCode::Char('c') if ctrl => return Some(Press::Exit),
    KeyCode::Char(ch) => ch.to_string(),
    other => {
      on.push(match other {
        KeyCode::Up => "upArrow",
        KeyCode::Down => "downArrow",
        KeyCode::Left => "leftArrow",
        KeyCode::Right => "rightArrow",
        KeyCode::PageUp => "pageUp",
        KeyCode::PageDown => "pageDown",
        KeyCode::Enter => "return",
        KeyCode::Esc => "escape",
        KeyCode::Tab | KeyCode::BackTab => "tab",
        KeyCode::Backspace => "backspace",
        KeyCode::Delete => "delete",
        _ => return None,
      });
      String::new()
    }
  };
  let key: ValueMap = KEYS.iter().map(|name| ((*name).to_owned(), Value::Bool(on.contains(name)))).collect();
  let mut press = ValueMap::default();
  press.insert("input".to_owned(), Value::Str(input.into()));
  press.insert("key".to_owned(), Value::Map(key));
  Some(Press::Input(Value::Map(press)))
}

/// A timer the page's mount effects start: the handler it steps, how often
/// and whether it fires again after the first time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
  pub handler: usize,
  pub every: Duration,
  pub repeats: bool,
}

/// The root page of a terminal application: lowered, with the state its
/// handler has stepped it to.
pub struct Session {
  app: PathBuf,
  module: String,
  library: Components,
  state: ValueMap,
  size: (u16, u16),
  written: Vec<String>,
}

impl Session {
  /// The page under `app` lowered; it fails to draw rather than to open when it is not Ink's.
  pub fn open(app: &Path) -> Result<Session, String> {
    Session::open_module(app, MODULE)
  }

  /// The component `module` names under `app`, `src/app.tsx#default` for one, as the root.
  pub fn open_module(app: &Path, module: &str) -> Result<Session, String> {
    Ok(Session { app: app.to_path_buf(), module: module.to_owned(), library: lower(app, module)?, state: ValueMap::default(), size: (80, 24), written: Vec::new() })
  }

  /// The page as it stands.
  pub fn view(&mut self) -> Result<Node, String> {
    self.step(None)
  }

  /// The page after its `useInput` handler runs on `press`; as it stands when it has none.
  pub fn press(&mut self, press: &Value) -> Result<Node, String> {
    self.step(Some(press))
  }

  /// Lowers the page again from disk, keeping the state.
  pub fn reload(&mut self) -> Result<(), String> {
    self.library = lower(&self.app, &self.module)?;
    Ok(())
  }

  /// The timers the page's mount effects start, in the order it starts them.
  pub fn timers(&self) -> Vec<Timer> {
    let Some(component) = self.library.get(&self.module) else { return Vec::new() };
    component
      .handlers
      .iter()
      .enumerate()
      .filter_map(|(handler, h)| {
        let (kind, ms) = h.event.split_once(':')?;
        let every = Duration::from_millis(ms.parse().ok()?);
        match kind {
          "interval" => Some(Timer { handler, every, repeats: true }),
          "timeout" => Some(Timer { handler, every, repeats: false }),
          _ => None,
        }
      })
      .collect()
  }

  /// The terminal's size, which the page reads through `useWindowSize` and `useStdout`.
  pub fn resize(&mut self, columns: u16, rows: u16) {
    self.size = (columns, rows);
  }

  /// Whether the page has called Ink's `useApp().exit()`.
  pub fn exited(&self) -> bool {
    self.state.get(snapfire_fsr_lower::EXIT_STATE) == Some(&Value::Bool(true))
  }

  /// The page after timer `handler` fires.
  pub fn fire(&mut self, handler: usize) -> Result<Node, String> {
    self.run(Some(HandlerRef::own(handler)), &Value::Null)
  }

  fn step(&mut self, press: Option<&Value>) -> Result<Node, String> {
    let handler = match press {
      Some(_) => self.library.get(&self.module).and_then(|component| component.handlers.iter().position(|handler| handler.event == "input")).map(HandlerRef::own),
      None => None,
    };
    let event = press.cloned().unwrap_or(Value::Null);
    if press.is_some() && handler.is_none() {
      return self.run(None, &Value::Null);
    }
    self.run(handler, &event)
  }

  fn run(&mut self, handler: Option<HandlerRef>, event: &Value) -> Result<Node, String> {
    let component = self.library.get(&self.module).ok_or_else(|| format!("{} did not lower", self.module))?.clone();
    let mut store = ValueMap::default();
    store.insert(snapfire_fsr_lower::TERMINAL_COLUMNS.to_owned(), Value::Int(self.size.0.into()));
    store.insert(snapfire_fsr_lower::TERMINAL_ROWS.to_owned(), Value::Int(self.size.1.into()));
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map(store));
    let stepped = Interpreter::default().island_step(&self.module, &component, &props, &self.state, handler, event, &self.library).map_err(|fail| fail.message)?;
    for (action, value) in &stepped.acts {
      if let ("$stdout" | "$stderr", Value::Str(text)) = (action.as_str(), value) {
        self.written.extend(text.trim_end_matches('\n').split('\n').map(str::to_owned));
      }
    }
    let excess = self.written.len().saturating_sub(WRITTEN);
    self.written.drain(..excess);
    self.state = stepped.state;
    let page = from_markup(&stepped.rendered.html)?;
    if self.written.is_empty() {
      return Ok(page);
    }
    Ok(Node::Box(Boxed::new().column().child(Text::plain(&self.written.join("\n"))).child(page)))
  }
}

/// The library `module` lowers to. A component it places that does not lower
/// refuses it, since a terminal has no browser to render that one in.
fn lower(app: &Path, module: &str) -> Result<Components, String> {
  let mut set = ComponentSet::new(app);
  set.lower(module).map_err(|e| e.to_string())?;
  if let Some((placed, why)) = set.browser_only.first() {
    return Err(format!("`{placed}` does not lower, so only a browser could render it: {why}"));
  }
  Ok(set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect())
}

/// The terminal held in raw mode on the alternate screen, put back on drop.
struct Screen;

impl Screen {
  fn enter() -> std::io::Result<Screen> {
    crossterm::terminal::enable_raw_mode()?;
    let mut out = std::io::stdout();
    out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J")?;
    out.flush()?;
    Ok(Screen)
  }
}

impl Drop for Screen {
  fn drop(&mut self) {
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    let _ = crossterm::terminal::disable_raw_mode();
  }
}

/// Runs the root page of `app` in this terminal until Ctrl-C or until it calls `exit`.
pub fn run(app: &Path) -> Result<(), BuildError> {
  let fail = |e: std::io::Error| BuildError::Dev(format!("terminal: {e}"));
  let mut session = Session::open(app).map_err(BuildError::Dev)?;
  let (tx, rx) = std::sync::mpsc::channel();
  let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
    if let Ok(event) = event {
      let built = event.paths.iter().all(|path| path.components().any(|part| matches!(part.as_os_str().to_str(), Some("generated" | "dist" | ".fsr"))));
      if !built {
        let _ = tx.send(());
      }
    }
  })
  .map_err(|e| BuildError::Dev(format!("terminal: watching {}: {e}", app.display())))?;
  watcher.watch(app, RecursiveMode::Recursive).map_err(|e| BuildError::Dev(format!("terminal: watching {}: {e}", app.display())))?;

  let screen = Screen::enter().map_err(fail)?;
  let (columns, rows) = terminal_size();
  session.resize(columns, rows);
  let mut shown = Frame::new(0, 0);
  let mut drawn = session.view();
  draw(&drawn, &mut shown, true).map_err(fail)?;
  let schedule = |session: &Session| -> Vec<(Timer, Instant)> { session.timers().into_iter().map(|timer| (timer, Instant::now() + timer.every)).collect() };
  let mut timers = schedule(&session);
  loop {
    let wait = timers.iter().map(|(_, at)| at.saturating_duration_since(Instant::now())).min().unwrap_or(IDLE).min(IDLE);
    if crossterm::event::poll(wait).map_err(fail)? {
      match crossterm::event::read().map_err(fail)? {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key_event(key) {
          Some(Press::Exit) => break,
          Some(Press::Input(press)) => {
            drawn = session.press(&press);
            if session.exited() {
              break;
            }
            draw(&drawn, &mut shown, false).map_err(fail)?;
          }
          None => {}
        },
        Event::Resize(..) => {
          let (columns, rows) = terminal_size();
          session.resize(columns, rows);
          drawn = session.view();
          draw(&drawn, &mut shown, true).map_err(fail)?;
        }
        _ => {}
      }
    }
    let now = Instant::now();
    let mut fired = false;
    timers.retain_mut(|(timer, at)| {
      if *at > now {
        return true;
      }
      drawn = session.fire(timer.handler);
      fired = true;
      *at = now + timer.every;
      timer.repeats
    });
    if session.exited() {
      break;
    }
    if fired {
      draw(&drawn, &mut shown, false).map_err(fail)?;
    }
    if rx.try_recv().is_ok() {
      while rx.try_recv().is_ok() {}
      drawn = session.reload().and_then(|()| session.view());
      timers = schedule(&session);
      draw(&drawn, &mut shown, false).map_err(fail)?;
    }
  }
  drop(screen);
  Ok(())
}

/// The terminal's columns and rows: 80 by 24 when it reports none.
fn terminal_size() -> (u16, u16) {
  match crossterm::terminal::size() {
    Ok((columns, rows)) if columns > 0 && rows > 0 => (columns, rows),
    _ => (80, 24),
  }
}

/// Draws `node` or the error in its place, writing what changed since
/// `shown`; `whole` clears the screen and writes every cell.
fn draw(node: &Result<Node, String>, shown: &mut Frame, whole: bool) -> std::io::Result<()> {
  let (columns, _) = terminal_size();
  let frame = match node {
    Ok(node) => render(node, columns),
    Err(why) => render(&Node::Text(Text::plain(&format!("fsr: {why}")).color(Color::Ansi(1))), columns),
  };
  let mut out = std::io::stdout();
  if whole {
    out.write_all(b"\x1b[2J")?;
    *shown = Frame::new(0, 0);
  }
  out.write_all(frame.diff(shown).as_bytes())?;
  out.flush()?;
  *shown = frame;
  Ok(())
}
