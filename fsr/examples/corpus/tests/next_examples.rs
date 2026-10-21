//! Every Next.js example migrated with `fsr migrate` and built, its outcome
//! held against `next_outcomes.txt`: `lowered`, the causes that sent a
//! component to the browser (`client:`), the error that refused the build
//! (`refused:`) or the router migrate does not handle yet (`unmigrated:`).
//! Each line ends with how many pages the migration routed to and how
//! many notes it left. `CORPUS_BLESS=1`
//! rewrites the file from this run; `CORPUS_NOTES=1` prints every note.

use std::path::{Path, PathBuf};

use corpus::copy;
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_migrate::{migrate, Source, Tree};

fn read_tree(dir: &Path, root: &Path, tree: &mut Tree) {
  let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
  entries.sort_by_key(|e| e.file_name());
  for entry in entries {
    let path = entry.path();
    if entry.file_type().unwrap().is_dir() {
      read_tree(&path, root, tree);
    } else {
      let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
      tree.insert(rel, std::fs::read(&path).unwrap());
    }
  }
}

/// `migrated` laid out over the corpus's React template, the tables the
/// migration asked for added to its configuration.
fn app_of(root: &Path, name: &str, migrated: &Tree, config: &[String], scratch: &Path) -> PathBuf {
  let dir = scratch.join(name);
  let _ = std::fs::remove_dir_all(&dir);
  copy(&root.join("template/app"), &dir.join("app"));
  std::fs::create_dir_all(dir.join("config")).unwrap();
  let mut toml = std::fs::read_to_string(root.join("template/app.toml")).unwrap();
  for table in config {
    toml.push('\n');
    toml.push_str(table);
  }
  std::fs::write(dir.join("config/app.toml"), toml).unwrap();
  for (path, bytes) in migrated.iter() {
    let target = dir.join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, bytes).unwrap();
  }
  dir.join("app")
}

fn outcome(app: &Path) -> String {
  match build(app, &Options::default()) {
    Err(e) => format!("refused: {}", e.to_string().lines().next().unwrap_or_default().replace(&app.display().to_string(), "<app>")),
    Ok(built) => {
      let mut causes: Vec<String> = built.report.causes.iter().map(|c| format!("{} {}", c.at, c.message)).collect();
      causes.sort();
      causes.dedup();
      if causes.is_empty() { "lowered".to_owned() } else { format!("client: {}", causes.join(" | ")) }
    }
  }
}

/// The applications in `example`: itself when it holds a `package.json`, the
/// directories up to two levels down that do when it is a monorepo.
fn apps_in(example: &Path) -> Vec<PathBuf> {
  if example.join("package.json").is_file() {
    return vec![example.to_path_buf()];
  }
  let mut found = Vec::new();
  let mut dirs = vec![(example.to_path_buf(), 0)];
  while let Some((dir, depth)) = dirs.pop() {
    for entry in std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.path().is_dir()) {
      if entry.path().join("package.json").is_file() {
        found.push(entry.path());
      } else if depth < 1 {
        dirs.push((entry.path(), depth + 1));
      }
    }
  }
  found.sort();
  found
}

/// The pages `migrated` routes to.
fn pages(migrated: &Tree) -> usize {
  migrated.paths().filter(|p| p.starts_with("app/routes/") && p.rsplit('/').next().is_some_and(|f| f.starts_with("page.") && !f.starts_with("page.loader."))).count()
}

#[test]
fn every_next_example_migrates_and_builds_to_the_outcome_it_is_held_to() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"));
  let scratch = std::env::temp_dir().join(format!("fsr-next-corpus-{}", std::process::id()));
  let mut examples: Vec<PathBuf> = std::fs::read_dir(root.join("next_examples")).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
  examples.sort();
  let print_notes = std::env::var_os("CORPUS_NOTES").is_some_and(|v| v == "1");
  let mut lines = Vec::new();
  for example in &examples {
    let apps = apps_in(example);
    if apps.is_empty() {
      lines.push(format!("{}\tno application", example.file_name().unwrap().to_string_lossy()));
    }
    for dir in apps {
    let name = dir.strip_prefix(root.join("next_examples")).unwrap().to_string_lossy().into_owned();
    let mut input = Tree::new();
    read_tree(&dir, &dir, &mut input);
    let (migrated, report) = migrate(&input);
    if print_notes {
      for note in &report.notes {
        eprintln!("{name}: {note}");
      }
    }
    let outcome = match report.source {
      Some(Source::NextApp | Source::NextPages) => {
        let app = app_of(root, &name.replace('/', "-"), &migrated, &report.config, &scratch);
        let outcome = outcome(&app);
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
        outcome
      }
      Some(other) => format!("unmigrated: {}", other.name()),
      None => "unmigrated: not a Next.js application".to_owned(),
    };
    lines.push(format!("{name}\t{outcome}\tpages {}\tnotes {}", pages(&migrated), report.notes.len()));
    }
  }
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root.join("next_outcomes.txt");
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
