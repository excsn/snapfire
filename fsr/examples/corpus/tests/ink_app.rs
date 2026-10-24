//! One Ink application outside the corpus, `INK_APP` naming its `app/` directory: prints its first frame at `INK_COLUMNS` by `INK_ROWS` (100 by 30), the frame after each step in `INK_KEYS`, the exit code once the page exits and the reason it is refused when it is. `INK_ARGS` is the command line its loader reads. A step is a character, with `\n` Enter, `\t` Tab, `\x1b` Esc and `\x7f` Backspace as a terminal sends them, or a name in braces: `{up}`, `{down}`, `{left}`, `{right}`, `{tab}`, `{shift-tab}`, `{enter}`, `{esc}`, `{backspace}`, `{delete}`, `{home}`, `{end}`, `{pageup}`, `{pagedown}` or `{resize 80x24}`. The processes the page spawns run to their close after each step and what they report reaches the page. With `INK_WAIT` set, each step is followed by that many milliseconds of the page's timers on a clock of the test's own. Skips, saying so, when `INK_APP` is unset.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, spawned, Press, Session, Timer};
use snapfire_fsr_terminal::Node;

/// How many timers one wait fires before it is taken for a loop.
const FIRES: usize = 10_000;

/// How many rounds of spawning one step runs before it is taken for a loop.
const SPAWN_ROUNDS: usize = 100;

enum Step {
  Key(String, KeyEvent),
  Resize(u16, u16),
}

fn steps(keys: &str) -> Vec<Step> {
  let mut out = Vec::new();
  let mut chars = keys.chars();
  while let Some(ch) = chars.next() {
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let event = match ch {
      '{' => {
        let name: String = chars.by_ref().take_while(|c| *c != '}').collect();
        if let Some(size) = name.strip_prefix("resize ") {
          let (columns, rows) = size.split_once('x').unwrap_or_else(|| panic!("`{{{name}}}` is `{{resize COLUMNSxROWS}}`"));
          out.push(Step::Resize(columns.trim().parse().expect("columns"), rows.trim().parse().expect("rows")));
          continue;
        }
        let event = match name.as_str() {
          "up" => key(KeyCode::Up),
          "down" => key(KeyCode::Down),
          "left" => key(KeyCode::Left),
          "right" => key(KeyCode::Right),
          "tab" => key(KeyCode::Tab),
          "shift-tab" => KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
          "enter" => key(KeyCode::Enter),
          "esc" => key(KeyCode::Esc),
          "backspace" => key(KeyCode::Backspace),
          "delete" => key(KeyCode::Delete),
          "home" => key(KeyCode::Home),
          "end" => key(KeyCode::End),
          "pageup" => key(KeyCode::PageUp),
          "pagedown" => key(KeyCode::PageDown),
          other => panic!("`{{{other}}}` is not a key INK_KEYS names"),
        };
        out.push(Step::Key(format!("{{{name}}}"), event));
        continue;
      }
      '\n' | '\r' => key(KeyCode::Enter),
      '\t' => key(KeyCode::Tab),
      '\u{1b}' => key(KeyCode::Esc),
      '\u{7f}' => key(KeyCode::Backspace),
      ch => key(KeyCode::Char(ch)),
    };
    out.push(Step::Key(format!("{ch:?}"), event));
  }
  out
}

/// Runs the timers due on `clock` up to `until`, in the order they fall due, each repeating one again after its interval while the page still holds it.
fn run_timers(session: &mut Session, timers: &mut Vec<(Timer, Duration)>, clock: &mut Duration, until: Duration) {
  for _ in 0..FIRES {
    timers.extend(session.take_started().into_iter().map(|timer| {
      let due = *clock + timer.every;
      (timer, due)
    }));
    let Some(next) = timers.iter().enumerate().filter(|(_, (_, due))| *due <= until).min_by_key(|(_, (_, due))| *due).map(|(at, _)| at) else { break };
    let (timer, due) = timers.remove(next);
    *clock = due;
    session.fire(&timer).unwrap_or_else(|why| panic!("a timer at {} ms: {why}", due.as_millis()));
    run_spawns(session);
    if timer.repeats && session.holds(&timer) {
      let again = due + timer.every;
      timers.push((timer, again));
    }
  }
  *clock = until;
}

/// Runs each process the page spawned to its close, handing the page what it reports, until no step spawns another.
fn run_spawns(session: &mut Session) {
  for _ in 0..SPAWN_ROUNDS {
    let spawns = session.take_spawns();
    if spawns.is_empty() {
      return;
    }
    for spawn in spawns {
      for (spawn, report, event) in spawned(spawn) {
        session.deliver(&spawn, report, &event).unwrap_or_else(|why| panic!("{} reporting: {why}", spawn.command));
      }
    }
  }
}

#[test]
fn an_ink_app_draws_its_first_frame() {
  let Some(app) = std::env::var_os("INK_APP") else {
    eprintln!("INK_APP is unset: no application to run, skipping");
    return;
  };
  let size = |name: &str, default: u16| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
  let (mut columns, rows) = (size("INK_COLUMNS", 100), size("INK_ROWS", 30));
  let args: Vec<String> = std::env::var("INK_ARGS").unwrap_or_default().split_whitespace().map(str::to_owned).collect();
  let mut session = match Session::open_with(std::path::Path::new(&app), snapfire_fsr_cli::terminal::params(&args)) {
    Ok(session) => session,
    Err(why) => panic!("refused: {why}"),
  };
  session.resize(columns, rows);
  let draw = |node: Node, columns: u16| snapfire_fsr_terminal::render(&node, columns).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>().join("\n");
  session.view().unwrap_or_else(|why| panic!("first frame: {why}"));
  run_spawns(&mut session);
  println!("{}", draw(session.view().unwrap_or_else(|why| panic!("first frame: {why}")), columns));
  let wait = Duration::from_millis(std::env::var("INK_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(0));
  let (mut timers, mut clock) = (Vec::new(), Duration::ZERO);
  for step in steps(&std::env::var("INK_KEYS").unwrap_or_default()) {
    let named = match step {
      Step::Resize(width, height) => {
        columns = width;
        session.resize(width, height);
        format!("{{resize {width}x{height}}}")
      }
      Step::Key(named, event) => {
        let Some(Press::Input(key)) = key_event(event) else { continue };
        session.press(&key).unwrap_or_else(|why| panic!("after {named}: {why}"));
        named
      }
    };
    run_spawns(&mut session);
    if !wait.is_zero() {
      let until = clock + wait;
      run_timers(&mut session, &mut timers, &mut clock, until);
    }
    let node = session.view().unwrap_or_else(|why| panic!("after {named}: {why}"));
    match wait.is_zero() {
      true => println!("--- after {named}\n{}", draw(node, columns)),
      false => println!("--- after {named} and {} ms\n{}", wait.as_millis(), draw(node, columns)),
    }
    if session.exited() {
      break;
    }
  }
  if session.exited() {
    println!("--- exited with {}", session.exit_code().unwrap_or(0));
  }
}
