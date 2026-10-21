//! One Ink application outside the corpus, `INK_APP` naming its `app/` directory: prints its first frame at `INK_COLUMNS` by `INK_ROWS` (100 by 30), the frame after each key in `INK_KEYS` and the reason it is refused when it is. Skips, saying so, when `INK_APP` is unset.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Session};

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
  for ch in std::env::var("INK_KEYS").unwrap_or_default().chars() {
    let code = match ch {
      '\n' => KeyCode::Enter,
      '\u{1b}' => KeyCode::Esc,
      ch => KeyCode::Char(ch),
    };
    let Some(Press::Input(key)) = key_event(KeyEvent::new(code, KeyModifiers::NONE)) else { continue };
    let node = session.press(&key).unwrap_or_else(|why| panic!("after {ch:?}: {why}"));
    println!("--- after {ch:?}\n{}", draw(node));
  }
}
