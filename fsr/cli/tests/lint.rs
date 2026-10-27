use std::path::PathBuf;

use snapfire_fsr_cli::new::{NewOptions, create};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn project() -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-lint-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  root
}

#[test]
fn a_new_project_lints_clean() {
  let root = project();
  let report = snapfire_fsr_cli::lint::run(&root.join("app")).unwrap();
  assert!(report.files > 0, "the scaffold's routes are linted");
  assert!(report.findings.is_empty(), "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_finding_in_a_route_fails_and_vendored_packages_are_not_linted() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/about.ts"), "export const about = 1;\ndebugger;\n").unwrap();
  std::fs::create_dir_all(app.join("vendor/left-pad")).unwrap();
  std::fs::write(app.join("vendor/left-pad/index.js"), "debugger;\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let found: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.path.strip_prefix(&app).unwrap().to_owned())).collect();
  assert_eq!(found, [("eslint/no-debugger", PathBuf::from("routes/about.ts"))]);
  assert!(report.failed(false));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_projects_lint_file_sets_levels() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/about.ts"), "export const about = 1;\ndebugger;\n").unwrap();
  std::fs::write(root.join(snapfire_lint::CONFIG_FILE), "[rules]\n\"eslint/no-debugger\" = \"warn\"\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  assert_eq!((report.errors(), report.warnings()), (0, 1), "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}
