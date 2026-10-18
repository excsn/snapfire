//! `fsr serve --mount name=path`: a site mounted from a path for the run,
//! over whatever the shell's table says, with nothing written to the shell.

use std::path::PathBuf;

use snapfire_fsr_cli::serve;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn dir(tag: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-serve-{tag}-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated/contracts")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  dir
}

fn plan(json: &str) -> snapfire_fsr_plan::Manifest {
  snapfire_fsr_plan::Manifest::from_json(json).unwrap()
}

const PAGE: &str = r#"{ "version": 2, "routes": [ { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
  { "slot": "content", "node": { "id": 1, "module": "routes/index/page.tsx#default" } } ] } } ] }"#;

#[test]
fn a_mount_given_for_the_run_puts_a_site_from_a_path_in_the_table_and_the_host_reloads_the_same_way() {
  let shell = dir("shell");
  std::fs::write(shell.join("generated/plan.sexp"), plan(PAGE).to_sexpr()).unwrap();
  std::fs::write(shell.join("app.toml"), "[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"k\"\n").unwrap();
  let site = dir("site");
  std::fs::write(site.join("generated/plan.sexp"), plan(PAGE).namespaced("billing", "/billing", "shell#document").to_sexpr()).unwrap();
  std::fs::write(site.join("app.toml"), "[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"k\"\n\n[site]\nname = \"billing\"\nat = \"/billing\"\n").unwrap();

  let builder = serve::builder_with_mounts(&shell, &[("billing".to_owned(), site.clone())]).unwrap();
  let row = &builder.config().sites.as_ref().unwrap().mounts["billing"];
  assert_eq!(row.artifact.as_deref(), Some(site.display().to_string().as_str()));
  let host = snapfire_fsr_sites::mount_all(snapfire_fsr_sites::mountable(builder)).unwrap().build().unwrap();
  let report = host.report().to_string();
  assert!(report.contains("billing                at /billing from"), "{report}");
  assert!(report.contains("/billing"), "{report}");
  let reloaded = host.reload().unwrap().to_string();
  assert!(reloaded.contains("billing                at /billing from"), "the reloader lays the mount over again: {reloaded}");
  assert!(!std::fs::read_to_string(shell.join("app.toml")).unwrap().contains("[sites"), "nothing was written into the shell");
}
