//! Every example as a checkout holds it: copied without anything a build, a
//! test or a bundle wrote, then built, its specs run, booted from its sources,
//! bundled and booted from its deploy tree. An output left on disk by an
//! earlier run hides a defect every one of those steps would otherwise show.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, bundle, test, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

fn examples() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples").canonicalize().unwrap()
}

/// `from` copied to `to` as a checkout would hold it: the files git tracks or would track, nothing it ignores.
fn checkout(from: &Path, to: &Path) {
  let listed = std::process::Command::new("git").current_dir(from).args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"]).output().unwrap();
  assert!(listed.status.success(), "git ls-files in {}: {}", from.display(), String::from_utf8_lossy(&listed.stderr));
  for file in String::from_utf8(listed.stdout).unwrap().split('\0').filter(|f| !f.is_empty()) {
    let target = to.join(file);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::copy(from.join(file), &target).unwrap();
  }
  copy_types(from, to);
}

/// Every `app/types/` under `from` copied to the same place under `to`: a
/// shell's and each of its sites'.
fn copy_types(from: &Path, to: &Path) {
  copy_dir(&from.join("app/types"), &to.join("app/types"));
  let Ok(entries) = std::fs::read_dir(from) else { return };
  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    if entry.path().is_dir() && !["app", "target", "node_modules"].contains(&name.as_str()) && !name.starts_with('.') {
      copy_types(&entry.path(), &to.join(&name));
    }
  }
}

/// `from` copied whole into `to`, when it exists. An example's `app/types/` is
/// ignored by git and written by `fsr types`, which fetches; the checkout takes
/// the source tree's so the typecheck sees what a developer's would.
fn copy_dir(from: &Path, to: &Path) {
  let Ok(entries) = std::fs::read_dir(from) else { return };
  std::fs::create_dir_all(to).unwrap();
  for entry in entries.flatten() {
    let path = entry.path();
    match path.is_dir() {
      true => copy_dir(&path, &to.join(entry.file_name())),
      false => {
        std::fs::copy(&path, to.join(entry.file_name())).unwrap();
      }
    }
  }
}

/// Every `config/app.toml` under `root`, a site's beside its shell's; sorted so a shell precedes its sites.
fn configs(root: &Path) -> Vec<PathBuf> {
  let mut found = Vec::new();
  let mut stack = vec![root.to_path_buf()];
  while let Some(dir) = stack.pop() {
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
      let path = entry.path();
      let name = entry.file_name().to_string_lossy().into_owned();
      if !path.is_dir() || ["target", "node_modules", "deploy", "app"].contains(&name.as_str()) || name.starts_with('.') {
        continue;
      }
      if path.join("config/app.toml").is_file() {
        found.push(path.join("config/app.toml"));
      }
      stack.push(path);
    }
  }
  found.sort();
  found
}

/// Why `config` cannot boot headless from its tree alone, when it cannot.
fn skipped(example: &Path, config: &str) -> Option<&'static str> {
  if !example.join("app").is_dir() {
    return Some("it has no app directory; its binary builds its own host");
  }
  if config.lines().any(|l| l.starts_with("[site]")) {
    return Some("a site ships inside its shell's tree");
  }
  if config.lines().any(|l| l.trim() == "runs = \"terminal\"") {
    return Some("a terminal program serves nothing");
  }
  if config.lines().any(|l| l.starts_with("[sites")) {
    return Some("a shell needs its site artifacts placed beside the tree");
  }
  let mut stack = vec![example.join("src")];
  while let Some(dir) = stack.pop() {
    for path in std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()) {
      if path.is_dir() {
        stack.push(path);
      } else if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(".extension(")) {
        return Some("its extensions are registered by the binary");
      }
    }
  }
  None
}

/// The host booted from the configuration under `root` and `/` rendered by it.
fn boot_and_render(root: &Path) -> (Result<(), String>, Result<(), String>) {
  let host = Config::load(root)
    .map_err(|e| e.to_string())
    .map(|mut config| {
      if config.session.key.is_none() {
        config.session.key = Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned());
      }
      config
    })
    .and_then(|config| Host::from_config(config).map_err(|e| e.to_string()))
    .and_then(|builder| builder.build().map_err(|e| e.to_string()));
  let host = match host {
    Ok(host) => host,
    Err(e) => return (Err(e), Err("not booted".to_owned())),
  };
  let rendered = tokio::runtime::Runtime::new()
    .unwrap()
    .block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default()))
    .map(|_| ())
    .map_err(|e| e.to_string());
  (Ok(()), rendered)
}

#[test]
fn every_example_builds_tests_and_boots_from_a_checkout() {
  let scratch = std::env::temp_dir().join(format!("fsr-examples-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&scratch);
  let examples = examples();
  for entry in std::fs::read_dir(&examples).unwrap().flatten().filter(|e| e.path().is_dir()) {
    let name = entry.file_name().to_string_lossy().into_owned();
    if name == "target" {
      continue;
    }
    if name == "conformance" {
      for inner in std::fs::read_dir(entry.path()).unwrap().flatten().filter(|e| e.path().is_dir()) {
        checkout(&inner.path(), &scratch.join("conformance").join(inner.file_name()));
      }
      continue;
    }
    checkout(&entry.path(), &scratch.join(&name));
  }
  let configs = configs(&scratch);
  assert!(configs.len() > 10, "{configs:?}");

  let mut failed = Vec::new();
  let mut booted = 0;
  let mut tested = 0;
  for config_path in configs {
    let project = config_path.parent().unwrap().parent().unwrap();
    let name = project.strip_prefix(&scratch).unwrap().to_string_lossy().into_owned();
    let config = std::fs::read_to_string(&config_path).unwrap();
    let app = project.join("app");
    if !app.is_dir() {
      println!("{name}: skipped, it has no app directory; its binary builds its own host");
      continue;
    }
    let built = match build(&app, &Options::beside(&app)) {
      Ok(built) => built,
      Err(e @ snapfire_fsr_cli::BuildError::MdxFeature { .. }) => {
        println!("{name}: skipped, {e}");
        continue;
      }
      Err(e) => {
        failed.push(format!("{name}: did not build: {e}"));
        continue;
      }
    };
    write(&app, &built).unwrap();
    if app.join("tests").is_dir() {
      match test::run(&app, &Options::beside(&app), &snapfire_fsr_cli::typecheck::Typecheck::beside(&app), None) {
        Ok(summary) if summary.failed == 0 => {
          tested += 1;
          println!("{name}: {} specs passed", summary.passed);
        }
        Ok(summary) => {
          let lines: Vec<&String> = summary.lines.iter().filter(|line| line.contains("FAILED")).collect();
          failed.push(format!("{name}: {} specs failed\n{}", summary.failed, lines.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n")));
          continue;
        }
        Err(e) => {
          failed.push(format!("{name}: fsr test did not run: {e}"));
          continue;
        }
      }
    }
    if let Some(why) = skipped(project, &config) {
      println!("{name}: not booted, {why}");
      continue;
    }

    let (source_boot, source_render) = boot_and_render(project);
    if let Err(e) = &source_boot {
      failed.push(format!("{name}: did not boot from its sources: {e}"));
      continue;
    }
    let tree = project.join("tree");
    if let Err(e) = bundle::run_checked(&app, &tree, false) {
      failed.push(format!("{name}: did not bundle: {e}"));
      continue;
    }
    let (tree_boot, tree_render) = boot_and_render(&tree);
    match (&tree_boot, &source_render, &tree_render) {
      (Err(e), _, _) => failed.push(format!("{name}: booted from its sources, not from its tree: {e}")),
      (Ok(()), Ok(()), Err(e)) => failed.push(format!("{name}: rendered / from its sources, not from its tree: {e}")),
      _ => {
        booted += 1;
        println!("{name}: booted from its tree, / {}", if source_render.is_ok() { "rendered" } else { "unrendered from either" });
      }
    }
  }
  let _ = std::fs::remove_dir_all(&scratch);
  assert!(failed.is_empty(), "{}", failed.join("\n"));
  assert!(tested > 8, "only {tested} examples ran their specs");
  assert!(booted > 5, "only {booted} examples booted");
}
