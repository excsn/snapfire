//! Every react.dev example `outcomes.txt` does not hold as `lowered`, built
//! again after `fsr migrate`'s lift: what a component awaits while it renders
//! moved to the route's loader. The outcome is held against
//! `migrated_outcomes.txt` the way `tests/corpus.rs` holds the rest;
//! `CORPUS_BLESS=1` rewrites the file from this run. A line marked `eager`
//! moved a read only a later render makes, which the first render never
//! asked for, so it is not counted as migrated.

use std::path::{Path, PathBuf};

use corpus::app_of;
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_migrate::{lift, Tree};

fn outcome(app: &Path) -> String {
  match build(app, &Options::default()) {
    Err(e) => format!("refused: {}", e.to_string().lines().next().unwrap_or_default().replace(&app.display().to_string(), "<app>")),
    Ok(built) => {
      let mut causes: Vec<String> = built.report.causes.iter().filter(|c| c.at.starts_with("src/")).map(|c| format!("{} {}", c.at, c.message)).collect();
      causes.sort();
      causes.dedup();
      if causes.is_empty() { "lowered".to_owned() } else { format!("client: {}", causes.join(" | ")) }
    }
  }
}

fn read_tree(dir: &Path, root: &Path, tree: &mut Tree) {
  for entry in std::fs::read_dir(dir).unwrap().flatten() {
    let path = entry.path();
    if entry.file_type().unwrap().is_dir() {
      if entry.file_name() != "vendor" {
        read_tree(&path, root, tree);
      }
    } else {
      tree.insert(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read(&path).unwrap());
    }
  }
}

#[test]
fn every_example_left_to_the_browser_builds_after_the_lift_to_the_outcome_it_is_held_to() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"));
  let scratch = std::env::temp_dir().join(format!("fsr-corpus-migrated-{}", std::process::id()));
  let held_outcomes = std::fs::read_to_string(root.join("outcomes.txt")).unwrap();
  let mut lines = Vec::new();
  for line in held_outcomes.lines() {
    let Some((name, held)) = line.split_once('\t') else { continue };
    if held == "lowered" {
      continue;
    }
    let example: PathBuf = root.join("react_dev").join(name);
    let app = app_of(root, &example, &scratch);
    let project = app.parent().unwrap().to_path_buf();
    let mut tree = Tree::new();
    read_tree(&project, &project, &mut tree);
    let (lifted, report) = lift(&tree);
    let outcome = if lifted == tree {
      "unchanged".to_owned()
    } else {
      for (path, bytes) in lifted.iter() {
        let target = project.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
      }
      outcome(&app)
    };
    let eager = report.notes.iter().any(|n| n.message.contains("loads with the page before any render reads it"));
    lines.push(format!("{name}\t{outcome}{}\tnotes {}", if eager { "\teager" } else { "" }, report.notes.len()));
    let _ = std::fs::remove_dir_all(&project);
  }
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root.join("migrated_outcomes.txt");
  if std::env::var_os("CORPUS_BLESS").is_some_and(|v| v == "1") {
    std::fs::write(&held_at, &actual).unwrap();
    return;
  }
  let held = std::fs::read_to_string(&held_at).unwrap_or_default();
  let changed: Vec<String> = held
    .lines()
    .zip(actual.lines())
    .filter(|(was, now)| was != now)
    .map(|(was, now)| format!("was {was}\n now {now}"))
    .collect();
  assert!(changed.is_empty() && held.lines().count() == actual.lines().count(), "{} outcomes changed; CORPUS_BLESS=1 takes this run:\n{}", changed.len(), changed.join("\n"));
}
