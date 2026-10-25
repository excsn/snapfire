//! Every vuejs.org example built as an application whose page places its
//! `App.vue` as an island, its outcome held against `vue_outcomes.txt`:
//! `lowered`, the causes that sent a component to the browser (`client:`) or
//! the error that refused the build (`refused:`). `CORPUS_BLESS=1` rewrites
//! the file from this run. `snapfirec-vue` has to be on `PATH`.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, Options};

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

/// `example` laid out as an application whose only page places its `App.vue`.
fn app_of(root: &Path, example: &Path, scratch: &Path) -> PathBuf {
  let dir = scratch.join(example.file_name().unwrap());
  let _ = std::fs::remove_dir_all(&dir);
  copy(&root.join("template_vue/app"), &dir.join("app"));
  std::fs::create_dir_all(dir.join("config")).unwrap();
  std::fs::copy(root.join("template_vue/app.toml"), dir.join("config/app.toml")).unwrap();
  copy(example, &dir.join("app/src"));
  std::fs::create_dir_all(dir.join("app/routes")).unwrap();
  std::fs::write(dir.join("app/routes/page.tsx"), "import { Island } from \"@snapfire/fsr/template\";\nimport App from \"@src/App.vue\";\n\nexport default function Page() {\n  return (\n    <Island>\n      <App />\n    </Island>\n  );\n}\n").unwrap();
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
fn every_vue_example_builds_to_the_outcome_it_is_held_to() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR"));
  let scratch = std::env::temp_dir().join(format!("fsr-vue-corpus-{}", std::process::id()));
  let mut examples: Vec<PathBuf> = std::fs::read_dir(root.join("vuejs_org")).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
  examples.sort();
  let mut lines = Vec::new();
  for example in &examples {
    let app = app_of(root, example, &scratch);
    lines.push(format!("{}\t{}", example.file_name().unwrap().to_string_lossy(), outcome(&app)));
    let _ = std::fs::remove_dir_all(app.parent().unwrap());
  }
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root.join("vue_outcomes.txt");
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
