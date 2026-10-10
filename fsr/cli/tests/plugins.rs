//! The plugin host across builds: a rebuild asks a plugin only about the files that changed.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::plugins::Plugins;
use snapfire_fsr_cli::{build_with, Options};

fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let name = entry.file_name();
    if ["target", "node_modules", ".fsr-test"].contains(&name.to_string_lossy().as_ref()) {
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

/// The recipes example's `.vue` files, described by `snapfirec-vue` from `PATH`.
#[test]
fn a_rebuild_describes_only_the_vue_files_that_changed() {
  let root = std::env::temp_dir().join(format!("fsr-plugins-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/recipes_vue_ts"), &root);
  let app: PathBuf = root.join("app");
  let mut plugins = Plugins::new();
  build_with(&app, &Options::beside(&app), &mut plugins).expect("the first build");
  let first = plugins.units_described();
  assert!(first >= 4, "every .vue file is described once: {first}");
  build_with(&app, &Options::beside(&app), &mut plugins).expect("the second build");
  assert_eq!(plugins.units_described(), first, "nothing changed, so nothing is described again");
  let scaler = app.join("src/ui/Scaler.vue");
  let source = std::fs::read_to_string(&scaler).unwrap().replace("<h3>Ingredients</h3>", "<h3>What goes in</h3>");
  std::fs::write(&scaler, source).unwrap();
  build_with(&app, &Options::beside(&app), &mut plugins).expect("the third build");
  assert_eq!(plugins.units_described(), first + 1, "only the edited file is described again");
  let _ = std::fs::remove_dir_all(&root);
}
