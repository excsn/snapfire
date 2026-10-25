//! `fsr run`: a terminal program run once, with no watcher.

use snapfire_fsr_cli::terminal::run_once;

fn project(config: &str, page: &str) -> std::path::PathBuf {
  static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-terminal-run-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), config).unwrap();
  std::fs::create_dir_all(root.join("app/routes")).unwrap();
  std::fs::write(root.join("app/routes/page.tsx"), page).unwrap();
  root.join("app")
}

#[test]
fn an_app_that_runs_on_the_web_is_refused_naming_fsr_serve() {
  let app = project("", "export default function Page() {\n  return <p>hi</p>;\n}\n");
  let error = run_once(&app, &[]).unwrap_err().to_string();
  assert!(error.contains("fsr serve") && error.contains("runs = \"terminal\""), "{error}");
}

#[test]
fn a_page_the_terminal_refuses_ends_the_run_with_the_reason() {
  let app = project("[app]\nruns = \"terminal\"\n", "export default function Page() {\n  return <p>hi</p>;\n}\n");
  let error = run_once(&app, &[]).unwrap_err().to_string();
  assert!(error.contains("is not an Ink element"), "the reason, not a wait for an edit: {error}");
}
