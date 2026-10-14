//! The theme names a build knows: one per file under `styles/themes/`, and the root's own.

use snapfire_fsr_cli::{theme_files, BuildError};

fn dir(files: &[&str]) -> std::path::PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-themes-{}-{}", std::process::id(), files.join("-")));
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap();
  for file in files {
    std::fs::write(root.join(file), "@theme {\n  --color-brand: #0e7490;\n}\n").unwrap();
  }
  root
}

#[test]
fn the_root_theme_is_named_default_once_there_are_themes() {
  assert_eq!(theme_files(&dir(&["ocean.css", "forest.css"])).unwrap(), ["default", "forest", "ocean"]);
  assert!(theme_files(&dir(&[])).unwrap().is_empty());
}

#[test]
fn a_theme_file_named_default_is_refused() {
  assert!(matches!(theme_files(&dir(&["default.css"])), Err(BuildError::ReservedTheme { .. })));
}
