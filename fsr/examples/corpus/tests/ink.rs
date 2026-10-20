//! Every example of Ink run as a terminal application of its own, its outcome
//! held against `ink_outcomes.txt`: `rendered` when its first frame draws, or
//! the reason it does not. `CORPUS_BLESS=1` rewrites the file from this run;
//! `CORPUS_FRAMES=1` prints each first frame.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::terminal::Session;

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

/// The component an example's `render(<Name … />)` mounts, and where that call starts and ends.
fn rendered_root(source: &str) -> Option<(String, usize, usize)> {
  let start = source.match_indices("render(").map(|(at, _)| at).find(|&at| at == 0 || source[..at].ends_with('\n'))?;
  let after = source[start + "render(".len()..].trim_start().strip_prefix('<')?;
  let name: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
  let mut depth = 0;
  for (offset, ch) in source[start..].char_indices() {
    match ch {
      '(' => depth += 1,
      ')' => {
        depth -= 1;
        if depth == 0 {
          let end = start + offset + 1;
          let end = if source[end..].starts_with(';') { end + 1 } else { end };
          return Some((name, start, end));
        }
      }
      _ => {}
    }
  }
  None
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
      if let Some((name, start, end)) = rendered_root(&source) {
        source.replace_range(start..end, &format!("export default {name};"));
        root = Some(path.file_name().unwrap().to_string_lossy().into_owned());
      }
    }
    std::fs::write(&path, source).unwrap();
  }
  let root = root.ok_or("no `render(<Component />)` to say what it draws")?;
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
    Ok(node) if !snapfire_fsr_terminal::render(&node, 80).lines().iter().all(String::is_empty) => {
      if std::env::var_os("CORPUS_FRAMES").is_some_and(|v| v == "1") {
        println!("== {}\n{}", example.file_name().unwrap().to_string_lossy(), snapfire_fsr_terminal::render(&node, 80).lines().join("\n"));
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
