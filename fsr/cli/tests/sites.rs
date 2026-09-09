use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::sites;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-sites-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// A shell and a site scaffolded side by side, so the relative paths a link
/// writes have somewhere to point.
fn pair(tag: &str) -> (PathBuf, PathBuf) {
  let base = root(tag);
  let shell = base.join("shell");
  let site = base.join("handbook");
  create(&shell, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  create(&site, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  (shell, site)
}

fn config(project: &Path) -> String {
  std::fs::read_to_string(project.join("config/app.toml")).unwrap()
}

#[test]
fn a_link_writes_both_halves_and_the_paths_point_at_each_other() {
  let (shell, site) = pair("link");
  let linked = sites::link(&shell, &site, "/examples/handbook", None).unwrap();

  assert_eq!(linked.name, "handbook");
  assert_eq!(linked.artifact, "../handbook");
  assert_eq!(linked.shell_json, "../shell/app/generated/shell.json");
  assert!(!linked.site_kept);

  let site_toml = config(&site);
  assert!(site_toml.contains("[site]"), "{site_toml}");
  assert!(site_toml.contains("name = \"handbook\""), "{site_toml}");
  assert!(site_toml.contains("at = \"/examples/handbook\""), "{site_toml}");
  assert!(site_toml.contains("shell = \"../shell/app/generated/shell.json\""), "{site_toml}");
  assert!(config(&shell).contains("[sites.handbook]"), "{}", config(&shell));

  let rows = sites::list(&shell).unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].name, "handbook");
  assert_eq!(rows[0].at.as_deref(), Some("/examples/handbook"));
  assert_eq!(rows[0].note.as_deref(), None, "the row resolves");

  assert!(linked.next.iter().any(|c| c.starts_with("fsr build") && c.contains("shell")), "{:?}", linked.next);
}

/// Every line is back except the blank ones at the end of the file, which the
/// removal collapses.
#[test]
fn an_unlink_takes_both_halves_back_out() {
  let (shell, site) = pair("unlink");
  let before_shell = config(&shell);
  let before_site = config(&site);

  sites::link(&shell, &site, "/handbook", None).unwrap();
  let unlinked = sites::unlink(&shell, "handbook", false).unwrap();

  assert_eq!(unlinked.name, "handbook");
  assert!(unlinked.site_config.is_some(), "the site's own [site] came out too");
  assert_eq!(config(&shell).trim_end(), before_shell.trim_end(), "the shell's configuration is back as it was");
  assert_eq!(config(&site).trim_end(), before_site.trim_end(), "the site's configuration is back as it was");
  assert!(sites::list(&shell).unwrap().is_empty());
}

#[test]
fn keep_site_leaves_the_site_a_site() {
  let (shell, site) = pair("keep");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let unlinked = sites::unlink(&shell, "handbook", true).unwrap();

  assert!(unlinked.site_config.is_none());
  assert!(config(&site).contains("[site]"), "{}", config(&site));
  assert!(!config(&shell).contains("[sites.handbook]"), "{}", config(&shell));
}

#[test]
fn a_shell_that_is_a_site_mounts_nothing() {
  let (shell, site) = pair("nested");
  sites::link(&shell, &site, "/handbook", None).unwrap();

  let third = root("nested-third");
  create(&third, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  let refused = sites::link(&site, &third, "/deeper", None).unwrap_err().to_string();
  assert!(refused.contains("a site cannot mount sites"), "{refused}");
}

#[test]
fn a_name_the_table_holds_is_refused_before_anything_is_written() {
  let (shell, site) = pair("twice");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let after_first = config(&shell);

  let refused = sites::link(&shell, &site, "/elsewhere", Some("handbook")).unwrap_err().to_string();
  assert!(refused.contains("already mounted"), "{refused}");
  assert_eq!(config(&shell), after_first, "a refused link writes nothing");
}

#[test]
fn a_site_already_naming_something_else_is_refused() {
  let (shell, site) = pair("conflict");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  sites::unlink(&shell, "handbook", true).unwrap();

  let refused = sites::link(&shell, &site, "/other", Some("other")).unwrap_err().to_string();
  assert!(refused.contains("already names site `handbook`"), "{refused}");
}

#[test]
fn the_host_s_own_rules_on_a_name_and_a_path_are_the_command_s() {
  let (shell, site) = pair("rules");
  for (at, name) in [("handbook", None), ("/handbook/", None), ("/{id}", None), ("/ok", Some("Handbook"))] {
    let refused = sites::link(&shell, &site, at, name).unwrap_err().to_string();
    assert!(refused.contains("must be"), "{at} {name:?}: {refused}");
  }
  assert!(!config(&shell).contains("[sites."), "{}", config(&shell));
}

#[test]
fn unlinking_a_name_the_table_does_not_hold_says_what_it_holds() {
  let (shell, site) = pair("missing");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let refused = sites::unlink(&shell, "nope", false).unwrap_err().to_string();
  assert!(refused.contains("`nope` is not mounted"), "{refused}");
  assert!(refused.contains("handbook"), "it names what is mounted: {refused}");
}

/// A shell whose table mounts a versioned artifact in its own cache.
fn pinnable() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = std::env::temp_dir().join(format!("fsr-pin-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(root.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n\n[sites.billing]\nartifact = \"billing@1.0.0\"\n").unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();

  let site = root.join("sites/billing/1.0.0");
  std::fs::create_dir_all(site.join("app/generated")).unwrap();
  std::fs::write(site.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"b\"\n[session]\nkey = \"k\"\n[site]\nname = \"billing\"\nat = \"/billing\"\n").unwrap();
  std::fs::write(site.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();
  root
}

#[test]
fn pinning_writes_the_hash_into_the_mount() {
  let shell = pinnable();
  let pinned = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(pinned.len(), 1);
  assert_eq!(pinned[0].name, "billing");
  assert!(pinned[0].was.is_none());
  assert!(pinned[0].moved());

  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert!(toml.contains(&format!("hash = \"{}\"", pinned[0].hash)), "{toml}");
  assert!(toml.contains("[sites.billing]"), "{toml}");
}

/// Pinning twice is one pin: the second run has nothing to write.
#[test]
fn pinning_again_holds_when_nothing_moved() {
  let shell = pinnable();
  let first = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  let again = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(again[0].was.as_deref(), Some(first[0].hash.as_str()));
  assert!(!again[0].moved(), "the hash moved without the artifact moving");
  assert_eq!(std::fs::read_to_string(shell.join("app.toml")).unwrap().matches("hash = ").count(), 1);
}

/// A changed artifact repins to the new content rather than appending a second key.
#[test]
fn repinning_replaces_the_hash_it_had() {
  let shell = pinnable();
  let before = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins")[0].hash.clone();
  std::fs::write(shell.join("sites/billing/1.0.0/app/generated/plan.sexp"), "(plan 2)\n(route / (node 0 shell#document))\n").unwrap();
  let after = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(after[0].was.as_deref(), Some(before.as_str()));
  assert_ne!(after[0].hash, before);
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert_eq!(toml.matches("hash = ").count(), 1, "{toml}");
  assert!(toml.contains(&after[0].hash), "{toml}");
}

/// A mount naming a path is a working tree, so it is never pinned.
#[test]
fn a_path_mount_is_not_pinned() {
  let shell = pinnable();
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  std::fs::write(shell.join("app.toml"), toml.replace("artifact = \"billing@1.0.0\"", "artifact = \"sites/billing/1.0.0\"")).unwrap();
  assert!(snapfire_fsr_cli::sites::pin(&shell, None).expect("pins").is_empty());
  assert!(!std::fs::read_to_string(shell.join("app.toml")).unwrap().contains("hash = "));
}

#[test]
fn pinning_one_mount_leaves_the_others() {
  let shell = pinnable();
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  std::fs::write(shell.join("app.toml"), format!("{toml}\n[sites.other]\nartifact = \"other@2.0.0\"\n")).unwrap();
  let site = shell.join("sites/other/2.0.0");
  std::fs::create_dir_all(site.join("app/generated")).unwrap();
  std::fs::write(site.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"o\"\n[session]\nkey = \"k\"\n[site]\nname = \"other\"\nat = \"/other\"\n").unwrap();
  std::fs::write(site.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();

  let pinned = snapfire_fsr_cli::sites::pin(&shell, Some("billing")).expect("pins");
  assert_eq!(pinned.len(), 1);
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert_eq!(toml.matches("hash = ").count(), 1, "{toml}");
}

#[test]
fn pinning_a_name_the_table_does_not_mount_says_so() {
  let shell = pinnable();
  let e = snapfire_fsr_cli::sites::pin(&shell, Some("nope")).unwrap_err().to_string();
  assert!(e.contains("`nope` is not mounted"), "{e}");
}
