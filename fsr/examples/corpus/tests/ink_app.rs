//! One Ink application outside the corpus, `INK_APP` naming its `app/` directory: prints its first frame at `INK_COLUMNS` by `INK_ROWS` (100 by 30), the frame after each key in `INK_KEYS` and the reason it is refused when it is. With `INK_WAIT` set, each key is followed by that many milliseconds of the page's timers on a clock of the test's own. Skips, saying so, when `INK_APP` is unset.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Session, Timer};

/// How many timers one wait fires before it is taken for a loop.
const FIRES: usize = 10_000;

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
    if timer.repeats && session.holds(&timer) {
      let again = due + timer.every;
      timers.push((timer, again));
    }
  }
  *clock = until;
}

#[test]
fn an_ink_app_draws_its_first_frame() {
  let Some(app) = std::env::var_os("INK_APP") else {
    eprintln!("INK_APP is unset: no application to run, skipping");
    return;
  };
  let size = |name: &str, default: u16| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
  let (columns, rows) = (size("INK_COLUMNS", 100), size("INK_ROWS", 30));
  let mut session = match Session::open(std::path::Path::new(&app)) {
    Ok(session) => session,
    Err(why) => panic!("refused: {why}"),
  };
  session.resize(columns, rows);
  let draw = |node| snapfire_fsr_terminal::render(&node, columns).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>().join("\n");
  println!("{}", draw(session.view().unwrap_or_else(|why| panic!("first frame: {why}"))));
  let wait = Duration::from_millis(std::env::var("INK_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(0));
  let (mut timers, mut clock) = (Vec::new(), Duration::ZERO);
  for ch in std::env::var("INK_KEYS").unwrap_or_default().chars() {
    let code = match ch {
      '\n' => KeyCode::Enter,
      '\u{1b}' => KeyCode::Esc,
      ch => KeyCode::Char(ch),
    };
    let Some(Press::Input(key)) = key_event(KeyEvent::new(code, KeyModifiers::NONE)) else { continue };
    let node = session.press(&key).unwrap_or_else(|why| panic!("after {ch:?}: {why}"));
    if wait.is_zero() {
      println!("--- after {ch:?}\n{}", draw(node));
      continue;
    }
    let until = clock + wait;
    run_timers(&mut session, &mut timers, &mut clock, until);
    let node = session.view().unwrap_or_else(|why| panic!("after {ch:?} and {} ms: {why}", wait.as_millis()));
    println!("--- after {ch:?} and {} ms\n{}", wait.as_millis(), draw(node));
  }
}
