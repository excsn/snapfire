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
