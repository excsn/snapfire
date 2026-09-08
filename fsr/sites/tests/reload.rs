use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-reload-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn plan(routes: &str) -> String {
  format!(r#"{{"version":2,"routes":[{routes}],"sources":[],"actions":[],"handlers":[],"components":[]}}"#)
}

fn route(pattern: &str) -> String {
  let module = pattern.trim_matches('/').replace('/', "-");
  let module = if module.is_empty() { "index".to_owned() } else { module };
  format!(
    r#"{{"pattern":"{pattern}","plan":{{"id":0,"module":"shell#document","children":[{{"slot":"content","node":{{"id":1,"module":"routes/{module}/page.tsx#default"}}}}]}}}}"#
  )
}

fn shell(root: &Path, routes: &str) {
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  let manifest = snapfire_fsr_plan::Manifest::from_json(&plan(routes)).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), manifest.to_sexpr()).unwrap();
  std::fs::write(
    root.join("app.toml"),
    "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n[sites.billing]\nartifact = \"billing@1.0.0\"\n",
  )
  .unwrap();
}

fn site(at: &Path, routes: &str) {
  std::fs::create_dir_all(at.join("app/generated")).unwrap();
  let manifest = snapfire_fsr_plan::Manifest::from_json(&plan(routes))
    .unwrap()
    .namespaced("billing", "/billing", "shell#document");
  std::fs::write(at.join("app/generated/plan.sexp"), manifest.to_sexpr()).unwrap();
  std::fs::write(
    at.join("app.toml"),
    "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[site]\nname = \"billing\"\nat = \"/billing\"\n",
  )
  .unwrap();
}

fn host(root: &Path) -> Host {
  let config = Config::load(root).unwrap();
  snapfire_fsr_sites::mountable(snapfire_fsr_sites::mount_all(Host::from_config(config).unwrap()).unwrap())
    .build()
    .unwrap()
}

fn patterns(host: &Host) -> Vec<String> {
  host.report().app.routes.iter().map(|r| r.0.clone()).collect()
}

#[test]
fn a_sites_reload_takes_the_site_from_disk_and_leaves_the_shell_as_it_booted() {
  let root = dir("frozen");
  shell(&root, &route("/"));
  let artifact = root.join("sites/billing/1.0.0");
  site(&artifact, &route("/"));
  let host = host(&root);
  assert!(patterns(&host).contains(&"/billing".to_owned()));
  assert_eq!(host.report().sites.len(), 1);

  // Both trees move under the running process. Only the site's may be read.
  let moved = snapfire_fsr_plan::Manifest::from_json(&plan(&format!("{},{}", route("/"), route("/shell-edit")))).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), moved.to_sexpr()).unwrap();
  site(&artifact, &format!("{},{}", route("/"), route("/invoices")));

  host.reload_sites().unwrap();
  let after = patterns(&host);
  assert!(
    after.contains(&"/billing/invoices".to_owned()),
    "the site was read again: {after:?}"
  );
  assert!(
    !after.contains(&"/shell-edit".to_owned()),
    "the shell was reread and must not have been: {after:?}"
  );
}

#[test]
fn a_sites_reload_that_is_refused_leaves_the_running_tables_serving() {
  let root = dir("refused");
  shell(&root, &route("/"));
  let artifact = root.join("sites/billing/1.0.0");
  site(&artifact, &route("/"));
  let host = host(&root);

  std::fs::write(artifact.join("app/generated/plan.sexp"), "(not a plan").unwrap();
  let e = host.reload_sites().unwrap_err().to_string();
  assert!(!e.is_empty());
  assert!(
    patterns(&host).contains(&"/billing".to_owned()),
    "the old tables still serve"
  );
  assert_eq!(host.report().sites.len(), 1);
}

#[test]
fn a_sites_reload_picks_up_a_version_the_table_moved_to() {
  let root = dir("moved");
  shell(&root, &route("/"));
  site(&root.join("sites/billing/1.0.0"), &route("/"));
  let host = host(&root);
  let first = host.report().sites[0].hash.clone();

  site(
    &root.join("sites/billing/1.1.0"),
    &format!("{},{}", route("/"), route("/plans")),
  );
  std::fs::write(
    root.join("app.toml"),
    "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n[sites.billing]\nartifact = \"billing@1.1.0\"\n",
  )
  .unwrap();

  // The table lives in the shell's configuration, which a sites reload holds
  // frozen, so the row a running host mounts cannot move under it.
  host.reload_sites().unwrap();
  assert_eq!(host.report().sites[0].version, "1.0.0");
  assert_eq!(host.report().sites[0].hash, first);
}

#[test]
fn a_host_with_no_mounter_refuses_a_sites_reload() {
  let root = dir("nomounter");
  shell(&root, &route("/"));
  site(&root.join("sites/billing/1.0.0"), &route("/"));
  let config = Config::load(&root).unwrap();
  let host = snapfire_fsr_sites::mount_all(Host::from_config(config).unwrap())
    .unwrap()
    .build()
    .unwrap();
  let e = host.reload_sites().unwrap_err().to_string();
  assert!(e.contains("sites_mounter"), "{e}");
}
