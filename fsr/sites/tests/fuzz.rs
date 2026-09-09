//! Coverage-guided fuzzing of what a layout is allowed to write. Under
//! `cargo test` these run as randomised checks against the saved corpus;
//! under `cargo bolero test --package snapfire_fsr_sites <name>` they run an
//! engine with coverage feedback.
//!
//! The property is the one the bundler leans on: a destination is joined onto
//! an output directory, so it must not be able to reach outside it however the
//! configuration is written.

use std::path::{Component, Path, PathBuf};

use bolero::check;
use snapfire_fsr_host::config::Config;

fn project(id: u64) -> PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  std::env::temp_dir().join(format!("fsr-fuzz-layout-{}-{id}-{nanos}", std::process::id()))
}

fn inside(root: &Path, to: &str) -> bool {
  let path = Path::new(to);
  path.is_relative()
    && path.components().all(|c| matches!(c, Component::Normal(_)))
    && root.join(path).starts_with(root)
}

#[test]
fn no_configuration_places_a_file_outside_the_tree() {
  let mut id = 0u64;
  check!()
    .with_type::<(String, String, String, String)>()
    .for_each(|(route, dir, plan, map)| {
      // TOML basic strings, so the generated configuration parses and the
      // fuzzer spends its bytes on paths rather than on quoting.
      let clean = |s: &String| s.replace(['"', '\\', '\n', '\r', '\0'], "");
      let (route, dir, plan, map) = (clean(route), clean(dir), clean(plan), clean(map));
      id += 1;
      let at = project(id);
      let Ok(()) = std::fs::create_dir_all(at.join("app/generated")) else { return };
      let _ = std::fs::write(at.join("app/generated/plan.sexp"), "(plan 2)");
      let _ = std::fs::write(at.join("app/importmap.json"), "{}");
      let text = format!(
        "[app]\ndir = \"app\"\n\n[session]\nkey = \"k\"\n\n[server]\nplan = \"{plan}\"\n\n[document]\nimport_map = \"{map}\"\n\n[[static]]\nroute = \"{route}\"\ndir = \"{dir}\"\n"
      );
      let _ = std::fs::write(at.join("app.toml"), text);

      if let Ok(config) = Config::load(&at) {
        if let Ok(laid) = snapfire_fsr_sites::layout(&at, &config) {
          for place in &laid.places {
            assert!(inside(&at, &place.to), "{} leaves the tree", place.to);
          }
          if let Ok(rows) = laid.rows() {
            for row in &rows {
              assert!(inside(&at, &row.path), "{} leaves the tree", row.path);
            }
          }
        }
      }
      let _ = std::fs::remove_dir_all(&at);
    });
}
