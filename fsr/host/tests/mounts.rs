//! `[sites.<name>]` over an address: the keys a mount in another process
//! carries and what the table refuses.

use std::path::PathBuf;

use snapfire_fsr_host::Config;

fn shell(table: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-mounts-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated")).unwrap();
  std::fs::write(dir.join("generated/plan.sexp"), "(plan 6)\n").unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::write(
    dir.join("app.toml"),
    format!("[app]\ndir = \".\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"test-key\"\n\n[sites]\n{table}"),
  )
  .unwrap();
  dir
}

fn refused(table: &str) -> String {
  let dir = shell(table);
  let e = Config::load(&dir).map(|_| ()).unwrap_err().to_string();
  std::fs::remove_dir_all(&dir).unwrap();
  e
}

#[test]
fn a_mount_over_addresses_carries_its_prefix_its_hosts_and_its_timings() {
  let dir = shell("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\", \"https://reports.internal\"]\ntimeout = \"500ms\"\nretry = \"30s\"\nsecret = \"shared\"\n");
  let config = Config::load(&dir).unwrap();
  let mount = &config.sites.as_ref().unwrap().mounts["reports"];
  assert!(mount.is_remote());
  assert_eq!(mount.artifact, None);
  assert_eq!(mount.at.as_deref(), Some("/reports"));
  assert_eq!(mount.over, ["http://127.0.0.1:8104", "https://reports.internal"]);
  assert_eq!(mount.timeout(), std::time::Duration::from_millis(500));
  assert_eq!(mount.retry(), std::time::Duration::from_secs(30));
  assert_eq!(mount.secret.as_deref(), Some("shared"));
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_mount_over_addresses_defaults_its_timings() {
  let dir = shell("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\"]\n");
  let config = Config::load(&dir).unwrap();
  let mount = &config.sites.as_ref().unwrap().mounts["reports"];
  assert_eq!(mount.timeout(), snapfire_fsr_host::MountConfig::DEFAULT_TIMEOUT);
  assert_eq!(mount.retry(), snapfire_fsr_host::MountConfig::DEFAULT_RETRY);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_table_refuses_a_row_with_two_homes_or_none_and_an_address_row_missing_its_prefix() {
  let both = refused("root = \"sites\"\n[sites.reports]\nartifact = \"reports@1.0.0\"\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\"]\n");
  assert!(both.contains("names both artifact and over"), "{both}");
  let neither = refused("[sites.reports]\nhash = \"abc\"\n");
  assert!(neither.contains("needs artifact or over"), "{neither}");
  let no_at = refused("[sites.reports]\nover = [\"http://127.0.0.1:8104\"]\n");
  assert!(no_at.contains("over needs at"), "{no_at}");
}

#[test]
fn the_table_refuses_an_artifacts_keys_on_an_address_row_and_an_addresss_keys_on_an_artifact_row() {
  let pinned = refused("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\"]\nhash = \"abc\"\n");
  assert!(pinned.contains("hash and allow_engine belong to an artifact"), "{pinned}");
  let timed = refused("root = \"sites\"\n[sites.reports]\nartifact = \"reports@1.0.0\"\ntimeout = \"2s\"\n");
  assert!(timed.contains("timeout belongs to a site over an address"), "{timed}");
}

#[test]
fn the_table_refuses_a_malformed_prefix_address_or_duration() {
  let prefix = refused("[sites.reports]\nat = \"reports/\"\nover = [\"http://127.0.0.1:8104\"]\n");
  assert!(prefix.contains("is not a prefix"), "{prefix}");
  let address = refused("[sites.reports]\nat = \"/reports\"\nover = [\"127.0.0.1:8104\"]\n");
  assert!(address.contains("is not an address"), "{address}");
  let trailing = refused("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104/\"]\n");
  assert!(trailing.contains("no trailing slash"), "{trailing}");
  let duration = refused("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\"]\nretry = \"soon\"\n");
  assert!(duration.contains("is not a duration"), "{duration}");
}
