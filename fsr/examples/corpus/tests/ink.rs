//! Every example of Ink run as a terminal application of its own, its outcome
//! held against `ink_outcomes.txt`: `rendered` when its first frame draws and
//! it steps on each of its timers and on a key, otherwise the reason it does not. Each process it spawns is handed output and an exit rather than started. `CORPUS_BLESS=1` rewrites the file from this run;
//! `CORPUS_FRAMES=1` prints each first frame and `CORPUS_FRAMES=2` the frame after the timers fire too.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use snapfire_fsr_cli::terminal::{key_event, Press, Report, Session};

const INK_SOURCE: &str = "'../../src/index.js'";

fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let target = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &target);
    } else {
      std::fs::copy(entry.path(), target).unwrap();
    }
  }
}

/// What an example's `render(element, options?)` mounts, and where the call starts and ends.
fn rendered_root(source: &str) -> Option<(String, usize, usize)> {
  let start = source.match_indices("render(").map(|(at, _)| at).find(|&at| source[..at].rsplit('\n').next().is_some_and(|line| line.trim().is_empty()))?;
  let open = start + "render(".len();
  let (mut depth, mut close, mut split) = (1, None, None);
  for (offset, ch) in source[open..].char_indices() {
    match ch {
      '(' | '{' | '[' => depth += 1,
      ')' | '}' | ']' => {
        depth -= 1;
        if depth == 0 {
          close = Some(open + offset);
          break;
        }
      }
      ',' if depth == 1 => split = Some(open + offset),
      _ => {}
    }
  }
  let close = close?;
  let element = match split {
    Some(comma) if source[comma + 1..close].trim().is_empty() || source[comma + 1..close].trim().starts_with('{') => &source[open..comma],
    _ => &source[open..close],
  };
  let end = if source[close + 1..].starts_with(';') { close + 2 } else { close + 1 };
  Some((element.trim().to_owned(), start, end))
}

/// `example` laid out as an application, Ink's own source imported as `ink`,
/// with the module id of the component its `render` call mounts, now that
/// module's default export.
fn app_of(example: &Path, scratch: &Path) -> Result<(PathBuf, String), String> {
  let dir = scratch.join(example.file_name().unwrap()).join("app");
  let _ = std::fs::remove_dir_all(&dir);
  copy(example, &dir.join("src"));
  let mut root = None;
  for entry in std::fs::read_dir(dir.join("src")).unwrap().flatten() {
    let path = entry.path();
    let Ok(mut source) = std::fs::read_to_string(&path) else { continue };
    source = source.replace(INK_SOURCE, "'ink'");
    if root.is_none() {
      if let Some((element, start, end)) = rendered_root(&source) {
        source.replace_range(start..end, "");
        source.push_str(&format!("\nexport default function InkExampleRoot() {{\n  return (\n    {element}\n  );\n}}\n"));
        root = Some(path.file_name().unwrap().to_string_lossy().into_owned());
      }
    }
    std::fs::write(&path, source).unwrap();
  }
  let root = root.ok_or("no `render(…)` to say what it draws")?;
  Ok((dir, format!("src/{root}#default")))
}

fn outcome(example: &Path, scratch: &Path) -> String {
  let first = |why: String| format!("refused: {}", why.lines().next().unwrap_or_default());
  let (app, module) = match app_of(example, scratch) {
    Ok(found) => found,
    Err(why) => return first(why),
  };
  let mut session = match Session::open_module(&app, &module) {
    Ok(session) => session,
    Err(why) => return first(why.replace(&app.display().to_string(), "<app>")),
  };
  match session.view() {
    Ok(node) if snapfire_fsr_terminal::render(&node, 80).lines().iter().any(|line| line.contains("sf-island:")) => "refused: draws a component in place of a browser's island".to_owned(),
    Ok(node) if !snapfire_fsr_terminal::render(&node, 80).lines().iter().all(String::is_empty) => {
      if std::env::var_os("CORPUS_FRAMES").is_some_and(|v| v == "1" || v == "2") {
        println!("== {}\n{}", example.file_name().unwrap().to_string_lossy(), snapfire_fsr_terminal::render(&node, 80).lines().join("\n"));
      }
      let mut stepped = None;
      for timer in session.take_started() {
        match session.fire(&timer) {
          Ok(node) => stepped = Some(node),
          Err(why) => return first(format!("a timer's step fails: {why}")),
        }
      }
      for spawn in session.take_spawns() {
        let mut exit = snapfire_fsr_core::ValueMap::default();
        exit.insert("code".to_owned(), snapfire_fsr_core::Value::Int(0));
        exit.insert("signal".to_owned(), snapfire_fsr_core::Value::Null);
        for (report, event) in [(Report::Stdout, snapfire_fsr_core::Value::Str("output\n".into())), (Report::Exit, snapfire_fsr_core::Value::Map(exit.clone())), (Report::Close, snapfire_fsr_core::Value::Map(exit))] {
          match session.deliver(&spawn, report, &event) {
            Ok(node) => stepped = Some(node),
            Err(why) => return first(format!("a process's {} step fails: {why}", report.name())),
          }
        }
      }
      if let (Some(node), true) = (stepped, std::env::var_os("CORPUS_FRAMES").is_some_and(|v| v == "2")) {
        println!("== {} after its timers\n{}", example.file_name().unwrap().to_string_lossy(), snapfire_fsr_terminal::render(&node, 80).lines().join("\n"));
      }
      let Some(Press::Input(space)) = key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)) else { unreachable!("a space is input") };
      if let Err(why) = session.press(&space) {
        return first(format!("a key's step fails: {why}"));
      }
      "rendered".to_owned()
    }
    Ok(_) => "refused: draws nothing".to_owned(),
    Err(why) => first(why),
  }
}

#[test]
fn every_ink_example_runs_to_the_outcome_it_is_held_to() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"));
  let scratch = std::env::temp_dir().join(format!("fsr-corpus-ink-{}", std::process::id()));
  let mut examples: Vec<PathBuf> = std::fs::read_dir(root.join("ink_examples")).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
  examples.sort();
  let lines: Vec<String> = examples.iter().map(|example| format!("{}\t{}", example.file_name().unwrap().to_string_lossy(), outcome(example, &scratch))).collect();
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root.join("ink_outcomes.txt");
  if std::env::var_os("CORPUS_BLESS").is_some_and(|v| v == "1") {
    std::fs::write(&held_at, &actual).unwrap();
    return;
  }
  let held = std::fs::read_to_string(&held_at).unwrap_or_default();
  let changed: Vec<String> = held.lines().zip(actual.lines()).filter(|(was, now)| was != now).map(|(was, now)| format!("was {was}\n now {now}")).collect();
  assert!(changed.is_empty() && held.lines().count() == actual.lines().count(), "{} outcomes changed; CORPUS_BLESS=1 takes this run:\n{}", changed.len(), changed.join("\n"));
}
