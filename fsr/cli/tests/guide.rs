//! The guide's chapters name only what the authoring declarations accept.

use std::path::Path;

/// The values `IslandProps`'s `mode` takes in the authoring template's declarations.
fn island_modes() -> Vec<String> {
  let declarations = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("embedded/authoring/template.d.ts")).unwrap();
  let line = declarations.lines().find(|line| line.trim_start().starts_with("mode?:")).expect("IslandProps declares mode");
  line.split('"').skip(1).step_by(2).map(str::to_owned).collect()
}

#[test]
fn every_island_mode_the_guide_names_is_one_the_declarations_accept() {
  let modes = island_modes();
  assert!(!modes.is_empty());
  let guide = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/guide");
  let mut refused = Vec::new();
  for entry in std::fs::read_dir(&guide).unwrap().flatten() {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) != Some("md") {
      continue;
    }
    let text = std::fs::read_to_string(&path).unwrap();
    for (at, _) in text.match_indices("mode=\"") {
      if text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '-' || c == '_') {
        continue;
      }
      let value: String = text[at + 6..].chars().take_while(|c| *c != '"').collect();
      if !modes.contains(&value) {
        refused.push(format!("{}: mode=\"{value}\"", path.file_name().unwrap().to_string_lossy()));
      }
    }
  }
  assert!(refused.is_empty(), "the declarations take mode {modes:?}: {refused:?}");
}
