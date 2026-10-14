//! `[sites.<name>]`: a site is mounted from an artifact and nothing else.

use std::path::PathBuf;

use snapfire_fsr_host::Config;

fn shell(table: &str) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-mounts-{}-{nanos}-{n}", std::process::id()));
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
fn the_table_refuses_a_row_without_an_artifact() {
  let neither = refused("[sites.reports]\nhash = \"abc\"\n");
  assert!(neither.contains("needs artifact"), "{neither}");
}

#[test]
fn the_table_refuses_an_address_row() {
  let over = refused("[sites.reports]\nat = \"/reports\"\nover = [\"http://127.0.0.1:8104\"]\n");
  assert!(over.contains("unknown field"), "{over}");
}
