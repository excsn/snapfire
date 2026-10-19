//! Every svelte.dev example built as an application whose page places its
//! `App.svelte` as an island, its outcome held against
//! `svelte_outcomes.txt`: `lowered`, the causes that sent a component to the
//! browser (`client:`) or the error that refused the build (`refused:`).
//! `CORPUS_BLESS=1` rewrites the file from this run. `snapfirec-svelte` has
//! to be on `PATH`.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, Options};

const COMPILER: &str = ".fsr/compilers/svelte/5.57.2/index.js";

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

/// `example` laid out as an application whose only page places its `App.svelte`.
fn app_of(root: &Path, example: &Path, scratch: &Path) -> PathBuf {
  let dir = scratch.join(example.file_name().unwrap());
  let _ = std::fs::remove_dir_all(&dir);
  copy(&root.join("template_svelte/app"), &dir.join("app"));
  std::fs::create_dir_all(dir.join("config")).unwrap();
  std::fs::copy(root.join("template_svelte/app.toml"), dir.join("config/app.toml")).unwrap();
  copy(example, &dir.join("app/src"));
  std::fs::create_dir_all(dir.join("app/routes")).unwrap();
  std::fs::write(dir.join("app/routes/page.tsx"), "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport App from \"@src/App.svelte\";\n\nexport default function Page() {\n  return (\n    <Island>\n      <App />\n    </Island>\n  );\n}\n").unwrap();
  dir.join("app")
}

fn outcome(app: &Path) -> String {
  match build(app, &Options::default()) {
    Err(e) => format!("refused: {}", e.to_string().lines().next().unwrap_or_default().replace(&app.display().to_string(), "<app>")),
    Ok(built) => {
      let mut causes: Vec<String> = built.report.causes.iter().chain(&built.report.foreign).filter(|c| c.at.starts_with("src/")).map(|c| format!("{} {}", c.at, c.message)).collect();
      causes.sort();
      causes.dedup();
      if causes.is_empty() { "lowered".to_owned() } else { format!("client: {}", causes.join(" | ")) }
    }
  }
}

#[test]
fn every_svelte_example_builds_to_the_outcome_it_is_held_to() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"));
  let scratch = std::env::temp_dir().join(format!("fsr-svelte-corpus-{}", std::process::id()));
  let mut examples: Vec<PathBuf> = std::fs::read_dir(root.join("svelte_dev")).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
  examples.sort();
  let mut compiler: Option<Vec<u8>> = None;
  let mut lines = Vec::new();
  for example in &examples {
    let app = app_of(root, example, &scratch);
    if let Some(bytes) = &compiler {
      std::fs::create_dir_all(app.join(COMPILER).parent().unwrap()).unwrap();
      std::fs::write(app.join(COMPILER), bytes).unwrap();
    }
    lines.push(format!("{}\t{}", example.file_name().unwrap().to_string_lossy(), outcome(&app)));
    if compiler.is_none() {
      compiler = std::fs::read(app.join(COMPILER)).ok();
    }
    let _ = std::fs::remove_dir_all(app.parent().unwrap());
  }
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root.join("svelte_outcomes.txt");
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
