//! `fsr dev --terminal`: the application's root page in the terminal, drawn
//! with Ink's elements and stepped by its `useInput` handler. The page renders
//! `ink-*` markup through the interpreter, which the terminal crate reads back
//! and draws; a key runs the handler over the held state and only the cells
//! that changed are written. A change under the app lowers the page again and
//! keeps the state.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use notify::{RecursiveMode, Watcher};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components, HandlerRef};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_terminal::{from_markup, render, Color, Frame, Node, Text};

use crate::BuildError;

/// The page a terminal application draws.
pub const MODULE: &str = "routes/page.tsx#default";

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

/// The root page of a terminal application: lowered, with the state its
/// handler has stepped it to.
pub struct Session {
  app: PathBuf,
  library: Components,
  state: ValueMap,
}

impl Session {
  /// The page under `app` lowered; it fails to draw rather than to open when it is not Ink's.
  pub fn open(app: &Path) -> Result<Session, String> {
    Ok(Session { app: app.to_path_buf(), library: lower(app)?, state: ValueMap::default() })
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
    self.library = lower(&self.app)?;
    Ok(())
  }

  fn step(&mut self, press: Option<&Value>) -> Result<Node, String> {
    let component = self.library.get(MODULE).ok_or_else(|| format!("{MODULE} did not lower"))?.clone();
    let handler = press.and_then(|_| component.handlers.iter().position(|handler| handler.event == "input")).map(HandlerRef::own);
    let event = press.cloned().unwrap_or(Value::Null);
    let stepped = Interpreter::default().island_step(MODULE, &component, &ValueMap::default(), &self.state, handler, &event, &self.library).map_err(|fail| fail.message)?;
    self.state = stepped.state;
    from_markup(&stepped.rendered.html)
  }
}

fn lower(app: &Path) -> Result<Components, String> {
  let mut set = ComponentSet::new(app);
  set.lower(MODULE).map_err(|e| e.to_string())?;
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

/// Runs the root page of `app` in this terminal until Ctrl-C.
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
  let mut shown = Frame::new(0, 0);
  let mut drawn = session.view();
  draw(&drawn, &mut shown, true).map_err(fail)?;
  loop {
    if crossterm::event::poll(Duration::from_millis(100)).map_err(fail)? {
      match crossterm::event::read().map_err(fail)? {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key_event(key) {
          Some(Press::Exit) => break,
          Some(Press::Input(press)) => {
            drawn = session.press(&press);
            draw(&drawn, &mut shown, false).map_err(fail)?;
          }
          None => {}
        },
        Event::Resize(..) => draw(&drawn, &mut shown, true).map_err(fail)?,
        _ => {}
      }
    }
    if rx.try_recv().is_ok() {
      while rx.try_recv().is_ok() {}
      drawn = session.reload().and_then(|()| session.view());
      draw(&drawn, &mut shown, false).map_err(fail)?;
    }
  }
  drop(screen);
  Ok(())
}

/// Draws `node` or the error in its place, writing what changed since
/// `shown`; `whole` clears the screen and writes every cell.
fn draw(node: &Result<Node, String>, shown: &mut Frame, whole: bool) -> std::io::Result<()> {
  let columns = crossterm::terminal::size().ok().map(|(columns, _)| columns).filter(|columns| *columns > 0).unwrap_or(80);
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
