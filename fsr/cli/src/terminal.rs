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
use snapfire_fsr_terminal::{from_markup, render, render_measured, Boxed, Color, Frame, Node, Text};

use crate::BuildError;

/// The page a terminal application draws.
pub const MODULE: &str = "routes/page.tsx#default";

/// How many lines the page wrote with `useStdout` or `useStderr` stay above it.
const WRITTEN: usize = 200;

/// How many times a step runs the mount handlers of components a mount handler rendered.
const MOUNT_PASSES: usize = 8;

/// How many times a step renders again for a focus that moved while it rendered.
const FOCUS_PASSES: usize = 3;

/// How many times a step renders again for boxes whose measured layout changed, so `useBoxMetrics` reads where they landed.
const METRIC_PASSES: usize = 3;

/// How long the loop waits for a key when no timer is due sooner.
const IDLE: Duration = Duration::from_millis(100);

/// How long the loop waits for a key while a process it started may still write.
const RUNNING: Duration = Duration::from_millis(16);

/// What a process the terminal started reports: the spawn, the kind [`Session::deliver`] takes and its event.
type Reported = (Spawn, &'static str, Value);

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
  if event.code == KeyCode::BackTab && !on.contains(&"shift") {
    on.push("shift");
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

/// A timer a component's mount effects start: the component, by its path
/// in the page, the handler it steps, how often and whether it fires again
/// after the first time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
  pub path: String,
  pub handler: usize,
  pub every: Duration,
  pub repeats: bool,
}

/// A process a mount effect spawned with `node:child_process`, which the
/// terminal starts and whose output and exit it hands back through
/// [`Session::deliver`]: the component that spawned it, the id its handlers
/// are named by, the command, its arguments and the options as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawn {
  pub path: String,
  pub id: String,
  pub command: String,
  pub args: Vec<String>,
  pub options: Value,
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
  components: Vec<(String, String)>,
  focus: Focus,
  focusables_seen: Vec<Value>,
  focus_acts: Vec<(String, Value)>,
  mounted: std::collections::HashSet<String>,
  started: Vec<Timer>,
  metrics: ValueMap,
  cursor: Option<(u16, u16)>,
  spawns: Vec<Spawn>,
}

/// Ink's focus: the focusables the page rendered last, in order, the one
/// focused and whether Tab moves it.
#[derive(Default)]
struct Focus {
  focusables: Vec<Focusable>,
  focused: Option<String>,
  disabled: bool,
}

struct Focusable {
  id: String,
  auto: bool,
  active: bool,
}

impl Focus {
  /// The active focusable after the focused one, or the first, as Ink's `focusNext` takes it; `back` goes the other way.
  fn step(&mut self, back: bool) {
    let active: Vec<&str> = self.focusables.iter().filter(|f| f.active).map(|f| f.id.as_str()).collect();
    let at = self.focused.as_deref().and_then(|id| active.iter().position(|a| *a == id));
    let next = match (at, back) {
      (None, false) => active.first(),
      (None, true) => active.last(),
      (Some(at), false) => active.get(at + 1).or(active.first()),
      (Some(at), true) => at.checked_sub(1).and_then(|before| active.get(before)).or(active.last()),
    };
    self.focused = next.map(|id| (*id).to_owned());
  }

  /// Takes the focusables a render bound: an autofocused one takes the focus when none holds it, and a focus whose focusable left or went inactive is dropped.
  fn rendered(&mut self, found: &[Value]) {
    self.focusables = found
      .iter()
      .filter_map(|value| {
        let Value::Map(map) = value else { return None };
        let id = match map.get("id")? {
          Value::Str(id) => id.to_string(),
          _ => return None,
        };
        Some(Focusable { id, auto: map.get("autoFocus") == Some(&Value::Bool(true)), active: map.get("isActive") != Some(&Value::Bool(false)) })
      })
      .collect();
    if self.focused.as_ref().is_some_and(|id| !self.focusables.iter().any(|f| f.active && f.id == *id)) {
      self.focused = None;
    }
    if self.focused.is_none() {
      self.focused = self.focusables.iter().find(|f| f.active && f.auto).map(|f| f.id.clone());
    }
  }

  /// Runs one of the acts `useFocus` and `useFocusManager` lower their functions to; false for any other act.
  fn act(&mut self, action: &str, input: &Value) -> bool {
    match action {
      "$focus" => {
        if let Value::Str(id) = input {
          if self.focusables.iter().any(|f| f.id == **id) {
            self.focused = Some(id.to_string());
          }
        }
      }
      "$focusNext" => self.step(false),
      "$focusPrevious" => self.step(true),
      "$focusEnable" => self.disabled = false,
      "$focusDisable" => self.disabled = true,
      _ => return false,
    }
    true
  }

  fn value(&self) -> Value {
    self.focused.as_ref().map_or(Value::Null, |id| Value::Str(id.as_str().into()))
  }
}

impl Session {
  /// The page under `app` lowered; it fails to draw rather than to open when it is not Ink's.
  pub fn open(app: &Path) -> Result<Session, String> {
    Session::open_module(app, MODULE)
  }

  /// The component `module` names under `app`, `src/app.tsx#default` for one, as the root.
  pub fn open_module(app: &Path, module: &str) -> Result<Session, String> {
    Ok(Session { app: app.to_path_buf(), module: module.to_owned(), library: lower(app, module)?, state: ValueMap::default(), size: (80, 24), written: Vec::new(), components: vec![(String::new(), module.to_owned())], focus: Focus::default(), focusables_seen: Vec::new(), focus_acts: Vec::new(), mounted: Default::default(), started: Vec::new(), metrics: ValueMap::default(), cursor: None, spawns: Vec::new() })
  }

  /// The page as it stands.
  pub fn view(&mut self) -> Result<Node, String> {
    self.run(None, &Value::Null)
  }

  /// The page after every `useInput` handler it renders runs on `press`, as Ink gives a key to each; as it stands when it has none.
  pub fn press(&mut self, press: &Value) -> Result<Node, String> {
    let key = |name: &str| matches!(press, Value::Map(press) if matches!(press.get("key"), Some(Value::Map(key)) if key.get(name) == Some(&Value::Bool(true))));
    if !self.focus.disabled {
      match (key("tab"), key("shift"), key("escape")) {
        (true, shift, _) => self.focus.step(shift),
        (false, _, true) => self.focus.focused = None,
        _ => {}
      }
    }
    let heard: Vec<HandlerRef> = self.handlers().filter(|(_, _, event)| *event == "input").map(|(path, index, _)| HandlerRef::at(path, index)).collect();
    if heard.is_empty() {
      return self.run(None, &Value::Null);
    }
    let mut drawn = Err(String::new());
    for handler in heard {
      drawn = self.run(Some(handler), press);
      drawn.as_ref().map_err(Clone::clone)?;
    }
    drawn
  }

  /// Each handler of each component the page rendered last: its path, its index and its event.
  fn handlers(&self) -> impl Iterator<Item = (String, usize, String)> + '_ {
    self.components.iter().filter_map(|(path, module)| Some((path, self.library.get(module)?))).flat_map(|(path, component)| component.handlers.iter().enumerate().map(move |(index, h)| (path.clone(), index, h.event.clone())))
  }

  /// Lowers the page again from disk, keeping the state.
  pub fn reload(&mut self) -> Result<(), String> {
    self.library = lower(&self.app, &self.module)?;
    self.mounted.clear();
    self.started.clear();
    Ok(())
  }

  /// The timers the mount effects of the components the page rendered last start, in the order they start them.
  pub fn timers(&self) -> Vec<Timer> {
    self
      .handlers()
      .filter_map(|(path, handler, event)| {
        let (kind, ms) = event.split_once(':')?;
        let every = Duration::from_millis(ms.parse().ok()?);
        match kind {
          "interval" => Some(Timer { path, handler, every, repeats: true }),
          "timeout" => Some(Timer { path, handler, every, repeats: false }),
          _ => None,
        }
      })
      .collect()
  }

  /// The terminal's size, which the page reads through `useWindowSize` and `useStdout`.
  pub fn resize(&mut self, columns: u16, rows: u16) {
    self.size = (columns, rows);
  }

  /// The processes started since the last call, in the order the page spawned them.
  pub fn take_spawns(&mut self) -> Vec<Spawn> {
    std::mem::take(&mut self.spawns)
  }

  /// The page after `spawn` reports `kind`: `stdout` or `stderr` with a chunk of text, `exit` or `close` with `{ code, signal }`, `error` with `{ message }`. As it stands when the page does not listen for it or the component that spawned it is gone.
  pub fn deliver(&mut self, spawn: &Spawn, kind: &str, event: &Value) -> Result<Node, String> {
    let event_name = format!("spawn:{}:{kind}", spawn.id);
    let listener = self.handlers().find(|(path, _, event)| *path == spawn.path && *event == event_name).map(|(path, index, _)| HandlerRef::at(path, index));
    match listener {
      Some(handler) => self.run(Some(handler), event),
      None => self.run(None, &Value::Null),
    }
  }

  /// Where the page last put the cursor with Ink's `setCursorPosition`, in cells from the corner of what it draws: `None` keeps it hidden.
  pub fn cursor(&self) -> Option<(u16, u16)> {
    let (x, y) = self.cursor?;
    Some((x, y.saturating_add(self.written.len().min(usize::from(u16::MAX)) as u16)))
  }

  /// Whether the page has called Ink's `useApp().exit()`.
  pub fn exited(&self) -> bool {
    self.state.get(snapfire_fsr_lower::EXIT_STATE) == Some(&Value::Bool(true))
  }

  /// The page after `timer` fires; as it stands when the component that started it is gone.
  pub fn fire(&mut self, timer: &Timer) -> Result<Node, String> {
    if !self.holds(timer) {
      return self.run(None, &Value::Null);
    }
    self.run(Some(HandlerRef::at(timer.path.clone(), timer.handler)), &Value::Null)
  }

  /// Whether the component that started `timer` rendered last time, so the timer still runs.
  pub fn holds(&self, timer: &Timer) -> bool {
    self.components.iter().any(|(path, _)| *path == timer.path)
  }

  /// The timers started since the last call: those of each component as it first renders, and those a handler started.
  pub fn take_started(&mut self) -> Vec<Timer> {
    std::mem::take(&mut self.started)
  }

  /// Steps `handler` on `event` and renders, then renders again while that moved the focus, so `isFocused` reads where it landed.
  fn run(&mut self, handler: Option<HandlerRef>, event: &Value) -> Result<Node, String> {
    let mut html = self.render(handler, event)?;
    for _ in 0..MOUNT_PASSES {
      let fresh: Vec<(String, String)> = self.components.iter().filter(|(path, _)| !self.mounted.contains(path)).cloned().collect();
      if fresh.is_empty() {
        break;
      }
      for (path, module) in fresh {
        self.mounted.insert(path.clone());
        let Some(component) = self.library.get(&module).cloned() else { continue };
        for (index, handler) in component.handlers.iter().enumerate() {
          let Some((kind, ms)) = handler.event.split_once(':') else {
            if handler.event == "mount" {
              html = self.render(Some(HandlerRef::at(path.clone(), index)), &Value::Null)?;
            }
            continue;
          };
          let Ok(ms) = ms.parse() else { continue };
          if let "interval" | "timeout" = kind {
            self.started.push(Timer { path: path.clone(), handler: index, every: Duration::from_millis(ms), repeats: kind == "interval" });
          }
        }
      }
    }
    for _ in 0..FOCUS_PASSES {
      let before = self.focus.focused.clone();
      let found = std::mem::take(&mut self.focusables_seen);
      self.focus.rendered(&found);
      let acts = std::mem::take(&mut self.focus_acts);
      for (action, input) in &acts {
        self.focus.act(action, input);
      }
      if self.focus.focused == before && acts.is_empty() {
        break;
      }
      html = self.render(None, &Value::Null)?;
    }
    let mut page = from_markup(&html)?;
    for _ in 0..METRIC_PASSES {
      if !html.contains("ink-ref=") {
        break;
      }
      let measured = measured(&page, self.size.0);
      if measured == self.metrics {
        break;
      }
      self.metrics = measured;
      html = self.render(None, &Value::Null)?;
      page = from_markup(&html)?;
    }
    if self.written.is_empty() {
      return Ok(page);
    }
    Ok(Node::Box(Boxed::new().column().child(Text::plain(&self.written.join("\n"))).child(page)))
  }

  /// One step of the page: its acts taken, its state held and its markup.
  fn render(&mut self, handler: Option<HandlerRef>, event: &Value) -> Result<String, String> {
    let component = self.library.get(&self.module).ok_or_else(|| format!("{} did not lower", self.module))?.clone();
    let mut store = ValueMap::default();
    store.insert(snapfire_fsr_lower::TERMINAL_COLUMNS.to_owned(), Value::Int(self.size.0.into()));
    store.insert(snapfire_fsr_lower::TERMINAL_ROWS.to_owned(), Value::Int(self.size.1.into()));
    store.insert(snapfire_fsr_lower::TERMINAL_FOCUS.to_owned(), self.focus.value());
    store.insert(snapfire_fsr_lower::TERMINAL_METRICS.to_owned(), Value::Map(self.metrics.clone()));
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map(store));
    let path = handler.as_ref().map(|handler| handler.path.clone()).unwrap_or_default();
    let stepped = Interpreter::default().island_step(&self.module, &component, &props, &self.state, handler, event, &self.library).map_err(|fail| fail.message)?;
    for (action, value) in stepped.acts {
      match (action.as_str(), &value) {
        ("$stdout" | "$stderr", Value::Str(text)) => self.written.extend(text.trim_end_matches('\n').split('\n').map(str::to_owned)),
        ("$timer", Value::Map(timer)) => {
          let module = stepped.components.iter().find(|(at, _)| *at == path).map(|(_, module)| module.clone());
          let component = module.and_then(|module| self.library.get(&module).cloned());
          let (Some(component), Some(Value::Str(event))) = (component, timer.get("event")) else { continue };
          let Some(index) = component.handlers.iter().position(|h| h.event == **event) else { continue };
          let ms = match timer.get("ms") {
            Some(Value::F64(ms)) => *ms as u64,
            Some(Value::Int(ms)) => *ms as u64,
            _ => 0,
          };
          self.started.push(Timer { path: path.clone(), handler: index, every: Duration::from_millis(ms), repeats: timer.get("repeats") == Some(&Value::Bool(true)) });
        }
        ("$spawn", Value::Map(spawn)) => {
          let text = |v: Option<&Value>| match v {
            Some(Value::Str(s)) => Some(s.to_string()),
            Some(Value::Int(n)) => Some(n.to_string()),
            Some(Value::F64(n)) if n.fract() == 0.0 && n.abs() < 1e15 => Some((*n as i64).to_string()),
            Some(Value::F64(n)) => Some(n.to_string()),
            _ => None,
          };
          let (Some(id), Some(command)) = (text(spawn.get("id")), text(spawn.get("command"))) else { continue };
          let args = match spawn.get("args") {
            Some(Value::Seq(args)) => args.iter().filter_map(|a| text(Some(a))).collect(),
            _ => Vec::new(),
          };
          self.spawns.push(Spawn { path: path.clone(), id, command, args, options: spawn.get("options").cloned().unwrap_or(Value::Null) });
        }
        (focus, _) if focus.starts_with("$focus") => self.focus_acts.push((action, value)),
        _ => {}
      }
    }
    let excess = self.written.len().saturating_sub(WRITTEN);
    self.written.drain(..excess);
    self.state = stepped.state;
    self.components = stepped.components;
    self.focusables_seen = stepped.focusables;
    self.cursor = stepped.cursor.as_ref().and_then(cell_of);
    Ok(stepped.rendered.html)
  }
}

/// `{ x, y }` as the cell it names; anything else hides the cursor.
fn cell_of(at: &Value) -> Option<(u16, u16)> {
  let Value::Map(at) = at else { return None };
  let cell = |key: &str| match at.get(key) {
    Some(Value::Int(n)) => u16::try_from(*n).ok(),
    Some(Value::F64(n)) if n.is_finite() && *n >= 0.0 => Some(n.trunc().min(f64::from(u16::MAX)) as u16),
    _ => None,
  };
  Some((cell("x")?, cell("y")?))
}

/// Each box a ref is on, by the ref's id, as `useBoxMetrics` reads it once `page` is laid out across `columns`.
fn measured(page: &Node, columns: u16) -> ValueMap {
  let mut out = ValueMap::default();
  for (id, m) in render_measured(page, columns).1 {
    let mut fields = ValueMap::default();
    for (key, n) in [("width", i128::from(m.width)), ("height", i128::from(m.height)), ("left", i128::from(m.left)), ("top", i128::from(m.top)), ("clientWidth", i128::from(m.client_width)), ("clientHeight", i128::from(m.client_height))] {
      fields.insert(key.to_owned(), Value::Int(n));
    }
    out.insert(id, Value::Map(fields));
  }
  out
}

/// The library `module` lowers to. A component it places that does not lower
/// refuses it, since a terminal has no browser to render that one in.
fn lower(app: &Path, module: &str) -> Result<Components, String> {
  let mut set = ComponentSet::new(app).for_terminal();
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
  draw(&drawn, &mut shown, true, session.cursor()).map_err(fail)?;
  let schedule = |session: &mut Session| -> Vec<(Timer, Instant)> { session.take_started().into_iter().map(|timer| (Instant::now() + timer.every, timer)).map(|(at, timer)| (timer, at)).collect() };
  let mut timers = schedule(&mut session);
  let (reports, reported) = std::sync::mpsc::channel::<Reported>();
  let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
  let launch = |session: &mut Session| {
    for spawn in session.take_spawns() {
      start(spawn, reports.clone(), running.clone());
    }
  };
  launch(&mut session);
  loop {
    let busy = running.load(std::sync::atomic::Ordering::Relaxed) > 0;
    let wait = timers.iter().map(|(_, at)| at.saturating_duration_since(Instant::now())).min().unwrap_or(IDLE).min(if busy { RUNNING } else { IDLE });
    if crossterm::event::poll(wait).map_err(fail)? {
      match crossterm::event::read().map_err(fail)? {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key_event(key) {
          Some(Press::Exit) => break,
          Some(Press::Input(press)) => {
            drawn = session.press(&press);
            if session.exited() {
              break;
            }
            draw(&drawn, &mut shown, false, session.cursor()).map_err(fail)?;
          }
          None => {}
        },
        Event::Resize(..) => {
          let (columns, rows) = terminal_size();
          session.resize(columns, rows);
          drawn = session.view();
          draw(&drawn, &mut shown, true, session.cursor()).map_err(fail)?;
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
      drawn = session.fire(timer);
      fired = true;
      *at = now + timer.every;
      timer.repeats && session.holds(timer)
    });
    while let Ok((spawn, kind, event)) = reported.try_recv() {
      drawn = session.deliver(&spawn, kind, &event);
      fired = true;
    }
    timers.extend(schedule(&mut session));
    launch(&mut session);
    if session.exited() {
      break;
    }
    if fired {
      draw(&drawn, &mut shown, false, session.cursor()).map_err(fail)?;
    }
    if rx.try_recv().is_ok() {
      while rx.try_recv().is_ok() {}
      drawn = session.reload().and_then(|()| session.view());
      timers = schedule(&mut session);
      draw(&drawn, &mut shown, false, session.cursor()).map_err(fail)?;
    }
  }
  drop(screen);
  Ok(())
}

/// Starts `spawn`'s process with its output piped and reports each chunk it writes, then its exit and close, on `reports`; one that cannot start reports an error as Node words it, `spawn <command> ENOENT` for a command not found. `running` counts it while it may still report.
fn start(spawn: Spawn, reports: std::sync::mpsc::Sender<Reported>, running: Arc<std::sync::atomic::AtomicUsize>) {
  use std::process::{Command, Stdio};
  let text = |key: &str| match &spawn.options {
    Value::Map(options) => match options.get(key) {
      Some(Value::Str(s)) => Some(s.to_string()),
      _ => None,
    },
    _ => None,
  };
  let shell = matches!(&spawn.options, Value::Map(options) if matches!(options.get("shell"), Some(Value::Bool(true))));
  let mut command = match shell {
    true => {
      let mut sh = Command::new("sh");
      sh.arg("-c").arg(std::iter::once(spawn.command.clone()).chain(spawn.args.iter().cloned()).collect::<Vec<_>>().join(" "));
      sh
    }
    false => {
      let mut direct = Command::new(&spawn.command);
      direct.args(&spawn.args);
      direct
    }
  };
  if let Some(cwd) = text("cwd") {
    command.current_dir(cwd);
  }
  if let Value::Map(options) = &spawn.options {
    if let Some(Value::Map(env)) = options.get("env") {
      command.env_clear();
      for (key, value) in env.iter() {
        if let Value::Str(value) = value {
          command.env(key, value.as_str());
        }
      }
    }
  }
  command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
  let mut child = match command.spawn() {
    Ok(child) => child,
    Err(e) => {
      let code = match e.kind() {
        std::io::ErrorKind::NotFound => "ENOENT".to_owned(),
        std::io::ErrorKind::PermissionDenied => "EACCES".to_owned(),
        _ => e.to_string(),
      };
      let mut error = ValueMap::default();
      error.insert("message".to_owned(), Value::Str(format!("spawn {} {code}", spawn.command).into()));
      let _ = reports.send((spawn, "error", Value::Map(error)));
      return;
    }
  };
  running.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let readers: Vec<std::thread::JoinHandle<()>> = [(child.stdout.take().map(|out| Box::new(out) as Box<dyn std::io::Read + Send>), "stdout"), (child.stderr.take().map(|err| Box::new(err) as Box<dyn std::io::Read + Send>), "stderr")]
    .into_iter()
    .filter_map(|(pipe, kind)| pipe.map(|pipe| (pipe, kind)))
    .map(|(mut pipe, kind)| {
      let (reports, spawn) = (reports.clone(), spawn.clone());
      std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut held = Vec::new();
        loop {
          match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
              held.extend_from_slice(&buf[..n]);
              let whole = match std::str::from_utf8(&held) {
                Ok(_) => held.len(),
                Err(e) if e.error_len().is_none() => e.valid_up_to(),
                Err(_) => held.len(),
              };
              let chunk: Vec<u8> = held.drain(..whole).collect();
              if !chunk.is_empty() && reports.send((spawn.clone(), kind, Value::Str(String::from_utf8_lossy(&chunk).into_owned().into()))).is_err() {
                break;
              }
            }
          }
        }
        if !held.is_empty() {
          let _ = reports.send((spawn, kind, Value::Str(String::from_utf8_lossy(&held).into_owned().into())));
        }
      })
    })
    .collect();
  std::thread::spawn(move || {
    for reader in readers {
      let _ = reader.join();
    }
    let status = child.wait();
    let mut exit = ValueMap::default();
    let (code, signal) = match status {
      Ok(status) => (status.code().map_or(Value::Null, |c| Value::Int(c.into())), signal_name(&status)),
      Err(_) => (Value::Null, Value::Null),
    };
    exit.insert("code".to_owned(), code);
    exit.insert("signal".to_owned(), signal);
    let _ = reports.send((spawn.clone(), "exit", Value::Map(exit.clone())));
    let _ = reports.send((spawn, "close", Value::Map(exit)));
    running.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
  });
}

/// The signal that ended a process, named as Node names it, `SIGTERM`; null when it exited.
fn signal_name(status: &std::process::ExitStatus) -> Value {
  #[cfg(unix)]
  {
    use std::os::unix::process::ExitStatusExt;
    if let Some(signal) = status.signal() {
      let name = match signal {
        1 => "SIGHUP".to_owned(),
        2 => "SIGINT".to_owned(),
        3 => "SIGQUIT".to_owned(),
        6 => "SIGABRT".to_owned(),
        9 => "SIGKILL".to_owned(),
        13 => "SIGPIPE".to_owned(),
        15 => "SIGTERM".to_owned(),
        other => format!("SIG{other}"),
      };
      return Value::Str(name.into());
    }
  }
  let _ = status;
  Value::Null
}

/// The terminal's columns and rows: 80 by 24 when it reports none.
fn terminal_size() -> (u16, u16) {
  match crossterm::terminal::size() {
    Ok((columns, rows)) if columns > 0 && rows > 0 => (columns, rows),
    _ => (80, 24),
  }
}

/// Draws `node` or the error in its place, writing what changed since
/// `shown`; `whole` clears the screen and writes every cell. The cursor shows at `cursor` and is hidden without one.
fn draw(node: &Result<Node, String>, shown: &mut Frame, whole: bool, cursor: Option<(u16, u16)>) -> std::io::Result<()> {
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
  match cursor {
    Some((x, y)) => write!(out, "\x1b[{};{}H\x1b[?25h", u32::from(y) + 1, u32::from(x) + 1)?,
    None => out.write_all(b"\x1b[?25l")?,
  }
  out.flush()?;
  *shown = frame;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn spawned(command: &str, args: &[&str]) -> Vec<(&'static str, Value)> {
    let spawn = Spawn { path: String::new(), id: "1".to_owned(), command: command.to_owned(), args: args.iter().map(|a| (*a).to_owned()).collect(), options: Value::Null };
    let (reports, reported) = std::sync::mpsc::channel();
    start(spawn, reports, Arc::new(std::sync::atomic::AtomicUsize::new(0)));
    reported.iter().map(|(_, kind, event)| (kind, event)).collect()
  }

  #[test]
  fn a_started_process_reports_its_output_then_its_exit_and_close() {
    let mut exit = ValueMap::default();
    exit.insert("code".to_owned(), Value::Int(0));
    exit.insert("signal".to_owned(), Value::Null);
    assert_eq!(spawned("echo", &["hi"]), [("stdout", Value::Str("hi\n".into())), ("exit", Value::Map(exit.clone())), ("close", Value::Map(exit))]);
  }

  #[test]
  fn a_command_that_is_not_there_reports_the_error_node_words() {
    let mut error = ValueMap::default();
    error.insert("message".to_owned(), Value::Str("spawn fsr-no-such-command ENOENT".into()));
    assert_eq!(spawned("fsr-no-such-command", &[]), [("error", Value::Map(error))]);
  }
}
