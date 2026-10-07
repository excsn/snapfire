//! Every example bundled and booted from its deploy tree beside a boot from
//! its sources. A bundle moves what the host reads at boot, so a tree the host
//! cannot boot or render is a defect neither the build's nor the host's own
//! tests see.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, bundle, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

fn examples() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples").canonicalize().unwrap()
}

fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let name = entry.file_name();
    if ["target", "node_modules"].contains(&name.to_string_lossy().as_ref()) {
      continue;
    }
    let path = entry.path();
    if path.is_dir() {
      copy(&path, &to.join(&name));
    } else {
      std::fs::copy(&path, to.join(&name)).unwrap();
    }
  }
}

/// Why `config` cannot boot headless from its tree alone, when it cannot.
fn skipped(example: &Path, config: &str) -> Option<&'static str> {
  if !example.join("app").is_dir() {
    return Some("it has no app directory; its binary builds its own host");
  }
  if config.lines().any(|l| l.starts_with("[site]")) {
    return Some("a site ships inside its shell's tree");
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
fn every_example_boots_and_renders_from_its_deploy_tree_as_from_its_sources() {
  let scratch = std::env::temp_dir().join(format!("fsr-examples-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&scratch);
  let mut configs: Vec<PathBuf> = std::fs::read_dir(examples())
    .unwrap()
    .flatten()
    .map(|e| e.path().join("config/app.toml"))
    .filter(|p| p.is_file())
    .collect();
  configs.sort();
  assert!(configs.len() > 10, "{configs:?}");

  let mut failed = Vec::new();
  let mut booted = 0;
  for config_path in configs {
    let source = config_path.parent().unwrap().parent().unwrap();
    let name = source.file_name().unwrap().to_string_lossy().into_owned();
    let config = std::fs::read_to_string(&config_path).unwrap();
    if let Some(why) = skipped(source, &config) {
      println!("{name}: skipped, {why}");
      continue;
    }
    let project = scratch.join(&name);
    copy(source, &project);
    let app = project.join("app");
    let built = match build(&app, &Options::beside(&app)) {
      Ok(built) => built,
      Err(e) => {
        failed.push(format!("{name}: did not build: {e}"));
        continue;
      }
    };
    write(&app, &built).unwrap();

    let (source_boot, source_render) = boot_and_render(&project);
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
  assert!(booted > 5, "only {booted} examples booted");
}
